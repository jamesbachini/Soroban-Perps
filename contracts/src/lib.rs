#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contractclient, contracttype, panic_with_error,
    Address, Env, Symbol, symbol_short, Vec, Map, String,
};
use sep_41_token::TokenClient;

// Storage keys
const PRICE: Symbol = symbol_short!("PRICE");
const ASSET: Symbol = symbol_short!("ASST");
const LEVERAGE: Symbol = symbol_short!("LEV");
const PUSD: Symbol = symbol_short!("PUSD");
const ORACLES: Symbol = symbol_short!("ORCL");
const LONG_POS: Symbol = symbol_short!("LONG");
const SHORT_POS: Symbol = symbol_short!("SHT");
const MARGIN_REQ: Symbol = symbol_short!("MREQ");
const POSITIONS: Symbol = symbol_short!("PTNS");
const TRADE_HISTORY: Symbol = symbol_short!("HIST");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    PositionOpen = 1,
    PositionNotOpen = 2,
    ZeroValue = 3,
    AboveMargin = 4,
}

#[derive(Clone)]
#[contracttype]
pub struct Position {
    pub value: i128,
    pub open_price: i128,
    pub close_price: i128,
    pub long: bool,
}

#[contract]
pub struct PerpContract;

#[contractclient(name = "FungibleTokenClient")]
pub trait FungibleToken {


}

#[contractimpl]
impl PerpContract {


    /// Initialize contract parameters
    pub fn initialize(env: Env, asset: String, leverage: i128, p_usd: Address, oracle: Address) {
        env.storage().instance().set(&ASSET, &asset);
        env.storage().instance().set(&LEVERAGE, &leverage);
        env.storage().instance().set(&PUSD, &p_usd);

        let mut oracles: Map<Address, bool> = Map::new(&env);
        oracles.set(oracle.clone(), true);
        env.storage().instance().set(&ORACLES, &oracles);
        env.storage().instance().set(&MARGIN_REQ, &i128::from(300));
        env.storage().instance().set(&LONG_POS, &0_i128);
        env.storage().instance().set(&SHORT_POS, &0_i128);

        let history: Vec<Position> = Vec::new(&env);
        env.storage().instance().set(&TRADE_HISTORY, &history);
    }

    /// Place a new trade
    pub fn place_trade(env: Env, trader: Address, value: i128, long: bool) {
        trader.require_auth();
        // Load or create positions map
        let mut positions: Map<Address, Position> = env
            .storage()
            .persistent()
            .get(&POSITIONS)
            .unwrap_or_else(|| Map::new(&env));
        // 2do check user doesn't already have a postion
        if value <= 0 {
            panic_with_error!(&env, ContractError::ZeroValue);
        }

        // Transfer in pUSD
        let p_usd: Address = env.storage().instance().get(&PUSD).unwrap();
        TokenClient::new(&env, &p_usd).transfer_from(
            &env.current_contract_address(),
            &trader,
            &env.current_contract_address(),
            &value,
        );
        // Calculate fee
        let fee = Self::calculate_fee(&env, value, long);
        let remaining = value - fee;

        // Update totals
        let mut total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        let mut total_short: i128 = env.storage().instance().get(&SHORT_POS).unwrap();
        if long {
            total_long += remaining;
            env.storage().instance().set(&LONG_POS, &total_long);
        } else {
            total_short += remaining;
            env.storage().instance().set(&SHORT_POS, &total_short);
        }

        // Store and persist position
        let price: i128 = env.storage().instance().get(&PRICE).unwrap_or(0_i128);
        let position = Position { value: remaining, open_price: price, close_price: 0, long };
        positions.set(trader.clone(), position);
        env.storage().persistent().set(&POSITIONS, &positions);

        env.events().publish((symbol_short!("PLACE"),), (trader, value, long));
    }

    /// Calculate fee for a trade
    pub fn calculate_fee(env: &Env, value: i128, long: bool) -> i128 {
        let mut fee: i128 = 0;
        let total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap_or(0_i128);
        let total_short: i128 = env.storage().instance().get(&SHORT_POS).unwrap_or(0_i128);
        if total_long > total_short && long {
            fee = value / 100; // 1%
        }
        if total_short > total_long && !long {
            fee = value / 100; // 1%
        }
        return fee;
    }

    /// Calculate current position value
    pub fn calculate_position(env: &Env, user: Address) -> i128 {
        let positions: Map<Address, Position> = env
            .storage()
            .persistent()
            .get(&POSITIONS)
            .unwrap_or_else(|| Map::new(env));
        let position = match positions.get(user.clone()) {
            Some(p) => p,
            None => return 0,
        };
        let price: i128 = env.storage().instance().get(&PRICE).unwrap_or(0_i128);
        let mut gain: i128 = 0;
        let mut loss: i128 = 0;
        if position.long {
            if price > position.open_price {
                gain = price - position.open_price;
            } else {
                loss = position.open_price - price;
            }
        } else {
            if price < position.open_price {
                gain = position.open_price - price;
            } else {
                loss = price - position.open_price;
            }
        }
        let leverage: i128 = env.storage().instance().get(&LEVERAGE).unwrap();
        let mut ret = position.value;
        let multiplier = (leverage * position.value) / position.open_price;
        if gain > 0 {
            ret = position.value + (gain * multiplier);
        } else if loss > 0 {
            if loss * leverage > position.value {
                return 0; // should we take into account collateral requirements?
            }
            ret = position.value - (loss * multiplier);
        }
        ret
    }

    /// Close an open trade
    pub fn close_trade(env: Env, trader: Address) {
        trader.require_auth();
        let mut positions: Map<Address, Position> = env
            .storage()
            .persistent()
            .get(&POSITIONS)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::PositionNotOpen));
        let position = positions.get(trader.clone()).unwrap();
        let ret_bal = Self::calculate_position(&env, trader.clone());

        // Update history
        let mut history: Vec<Position> = env.storage().instance().get(&TRADE_HISTORY).unwrap();
        let mut closed = position.clone();
        closed.close_price = env.storage().instance().get(&PRICE).unwrap_or(0_i128);
        history.push_back(closed.clone());
        env.storage().instance().set(&TRADE_HISTORY, &history);

        // Update totals and remove
        let mut total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        let mut total_short: i128 = env.storage().instance().get(&SHORT_POS).unwrap();
        if position.long {
            total_long -= position.value;
            env.storage().instance().set(&LONG_POS, &total_long);
        } else {
            total_short -= position.value;
            env.storage().instance().set(&SHORT_POS, &total_short);
        }
        positions.remove(trader.clone());
        env.storage().persistent().set(&POSITIONS, &positions);

        // Payout
        let p_usd: Address = env.storage().instance().get(&PUSD).unwrap();
        TokenClient::new(&env, &p_usd).transfer(
            &env.current_contract_address(),
            &trader,
            &ret_bal,
        );
    }

    /// Liquidate an under-margined position
    pub fn liquidate_position(env: Env, liquidator: Address, user: Address) {
        liquidator.require_auth();
        let mut positions: Map<Address, Position> = env
            .storage()
            .persistent()
            .get(&POSITIONS)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::PositionNotOpen));
        let position = positions.get(user.clone()).unwrap();
        let ret_bal = Self::calculate_position(&env, user.clone());
        let margin_req: i128 = env.storage().instance().get(&MARGIN_REQ).unwrap();

        let margin = ret_bal * 10000 / position.value;
        if margin >= margin_req {
            panic_with_error!(&env, ContractError::AboveMargin);
        }

        // Archive history
        let mut history: Vec<Position> = env.storage().instance().get(&TRADE_HISTORY).unwrap();
        let mut closed = position.clone();
        closed.close_price = env.storage().instance().get(&PRICE).unwrap_or(0_i128);
        history.push_back(closed.clone());
        env.storage().instance().set(&TRADE_HISTORY, &history);

        // Update totals and remove position
        let mut total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        let mut total_short: i128 = env.storage().instance().get(&SHORT_POS).unwrap();
        if position.long {
            total_long -= position.value;
            env.storage().instance().set(&LONG_POS, &total_long);
        } else {
            total_short -= position.value;
            env.storage().instance().set(&SHORT_POS, &total_short);
        }
        positions.remove(user.clone());
        env.storage().persistent().set(&POSITIONS, &positions);

        // Reward liquidator
        let reward = ret_bal / 3;
        if reward > 0 {
            let p_usd: Address = env.storage().instance().get(&PUSD).unwrap();
            TokenClient::new(&env, &p_usd).transfer(
                &env.current_contract_address(),
                &liquidator,
                &reward,
            );
            
        }
        env.events().publish((symbol_short!("LIQ"),), (user, liquidator, ret_bal));
    }
}

mod test;