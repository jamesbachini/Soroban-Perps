// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract Perp {
    uint public price; // futures price
    uint public spot; // underlying spot price
    string public asset; // ticker symbol
    uint public leverage; // 10 = 10x leverage
    address public pUSD; // base asset, stablecoin
    mapping(address => bool) public oracles;
    uint public long; // total long value
    uint public short; // total short value
    uint public feeAdjustment; // oracle can partially control fees
    uint public marginRequirement; // oracle can adjust margin req
    struct Position {
        uint value;
        uint openPrice;
        uint closePrice;
        bool long;
    }
    mapping(address => Position) public positions;
    Position[] public tradeHistory;
    event PlaceTrade(address trader, uint value, bool long);
    event CloseTrade(address trader, uint returnBalance);
    event Liquidation(address trader, address liquidator, uint value);
    
    constructor (string memory _asset, uint _leverage, address _pUSD, address _oracle) {
        asset = _asset;
        leverage = _leverage;
        pUSD = _pUSD;
        oracles[_oracle] = true;
        feeAdjustment = 10000;
        marginRequirement = 300; // bps 3%
    }

    function placeTrade(uint _value, bool _long) external {
        require(positions[msg.sender].value == 0, "Close position first");
        require(_value > 0, "No value for trade provided");
        IERC20(pUSD).transferFrom(msg.sender, address(this), _value);
        uint fees = calculateFee(_value, _long);
        uint remaining = _value - fees;
        if (_long == true) long += remaining;
        if (_long == false) short += remaining;
        positions[msg.sender] = Position(remaining, price, 0, _long);
        emit PlaceTrade(msg.sender, _value, _long);
    }

    function calculateFee(uint _value, bool _long) public view returns(uint) {
        uint fee = 0; // zero fees when taking other side
        if (long > short && _long == true) {
            uint slippage = _value * 10000 / short;
            fee = (short * feeAdjustment / long) + slippage;
        }
        if (short > long && _long == false) {
            uint slippage = _value * 10000 / long;
            fee = (long * feeAdjustment / short) + slippage;
        }
        uint safuFee = 0;
        if (fee > 0) safuFee = _value / fee;
        return safuFee;
    }

    function calculatePosition(address _user) public view returns(uint) {
        if (positions[_user].value == 0) return 0;
        uint gain;
        uint loss;
        if (positions[_user].long == true) {
            if (price > positions[_user].openPrice)
                gain = price - positions[_user].openPrice;
            else loss = positions[_user].openPrice - price;
        }
        if (positions[_user].long == false) {
            if (price < positions[_user].openPrice)
                gain = positions[_user].openPrice - price;
            else loss = price - positions[_user].openPrice;
        }
        uint multiplier = leverage * positions[_user].value / positions[_user].openPrice;
        uint returnBalance = positions[_user].value;
        if (gain > 0) returnBalance = positions[_user].value + (gain * multiplier);
        if ((loss * multiplier) > positions[_user].value) return 0;
        if (loss > 0) returnBalance = positions[_user].value - (loss * multiplier);
        return returnBalance;
    }

    function closeTrade() external returns (uint) {
        require(positions[msg.sender].value > 0, "Open position first");
        uint returnBalance = calculatePosition(msg.sender);
        positions[msg.sender].closePrice = price;
        tradeHistory.push(positions[msg.sender]);
        if (positions[msg.sender].long == true) long -= positions[msg.sender].value;
        if (positions[msg.sender].long == false) short -= positions[msg.sender].value;
        delete positions[msg.sender];
        uint contractBalance = IERC20(pUSD).balanceOf(address(this));
        if (returnBalance > contractBalance) returnBalance = contractBalance;
        if (returnBalance > 0) IERC20(pUSD).transfer(msg.sender, returnBalance);
        emit CloseTrade(msg.sender, returnBalance);
        return returnBalance;
    }

    function liquidatePosition(address _user) external {
        require(positions[_user].value > 0, "No one to liquidate");
        uint returnBalance = calculatePosition(_user);
        uint margin = returnBalance * 10000 / positions[_user].value;
        require (margin < marginRequirement, "Position above margin requirement");
        positions[_user].closePrice = price;
        tradeHistory.push(positions[_user]);
        if (positions[_user].long == true) long -= positions[_user].value;
        if (positions[_user].long == false) short -= positions[_user].value;
        delete positions[_user];
        emit Liquidation(_user, msg.sender, returnBalance);
        uint splitFee = returnBalance / 3;
        if (splitFee > 0) {
            IERC20(pUSD).transfer(msg.sender, splitFee);
        }
    }
}