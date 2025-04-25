# Soroban Perps

Simplified perpetual futures contract for Soroban

A decentralized perpetual futures contract implementation on the Soroban platform, enabling leveraged trading with a synthetic USD (pUSD) stablecoin. This contract supports long/short positions, dynamic fees, liquidation mechanisms, and real-time PNL tracking.

Original project ported from Solidity PerpFactory: https://github.com/jamesbachini/PerpFactory

## Features
- **Leveraged Trading**: Users can open positions with predefined leverage (set during initialization).
- **Position Management**: Track open/closed positions and real-time profit/loss.
- **Liquidation System**: Under-collateralized positions are liquidated, with a reward for liquidators.
- **Oracle Integration**: Relies on external oracles for price feeds (oracle addresses whitelisted at initialization).
- **Event Logging**: Emits events for trade execution and liquidation.

## Key Functions

### `initialize(env, asset, leverage, p_usd, oracle)`
Initializes the contract with core parameters:
- `asset`: Asset identifier (e.g., "BTC").
- `leverage`: Default leverage multiplier (e.g., `10` for 10x).
- `p_usd`: Address of the pUSD token contract.
- `oracle`: Trusted oracle address for price updates.

### `place_trade(env, trader, value, long)`
Opens a new leveraged position:
- `value`: Collateral amount in pUSD.
- `long`: `true` for long, `false` for short.
- Transfers `value` pUSD from trader, applies fees, and records position.

### `close_trade(env, trader)`
Closes the caller's open position, settles PNL, and returns remaining collateral.

### `liquidate_position(env, liquidator, user)`
Allows liquidators to close under-margined positions:
- Rewards liquidator with 33% of remaining position value.
- Requires position value < 3% margin (set by `MARGIN_REQ`).

### `calculate_position(env, user)`
Returns the current value of a user's position based on latest price.

## Storage Layout
| Key           | Type               | Description                          |
|---------------|--------------------|--------------------------------------|
| `PRICE`       | `i128`             | Latest asset price from oracle       |
| `ASSET`       | `String`           | Traded asset symbol                  |
| `LEVERAGE`    | `i128`             | Leverage multiplier (e.g., 10x)      |
| `PUSD`        | `Address`          | pUSD token contract address          |
| `ORACLES`     | `Map<Address,bool>`| Whitelisted oracle addresses         |
| `LONG_POS`    | `i128`             | Total value of open long positions   |
| `SHORT_POS`   | `i128`             | Total value of open short positions  |
| `MARGIN_REQ`  | `i128`             | Maintenance margin requirement (300=3%) |
| `POSITIONS`   | `Map<Address,Position>`| Active user positions           |
| `TRADE_HISTORY`| `Vec<Position>`    | Closed position archive              |

## Error Codes
| Code                  | Description                               |
|-----------------------|-------------------------------------------|
| `PositionOpen` (1)    | User already has an open position        |
| `PositionNotOpen` (2) | No open position to close                |
| `ZeroValue` (3)       | Attempted operation with zero value      |
| `AboveMargin` (4)      | Position not eligible for liquidation    |

## Events
- **`(PLACE, (trader, value, long))`**: Emitted on new trade.
- **`(LIQ, (user, liquidator, ret_bal))`**: Emitted on liquidation.

## Usage Example

1. **Initialize Contract**
```rust
initialize(
    env,
    String::from_str(&env, "BTC"),
    10, // 10x leverage
    p_usd_token_address,
    oracle_address
);
```

2. **Open Long Position**
```rust
place_trade(
    env,
    trader_address,
    100_0000000, // 100 pUSD
    true // long
);
```

3. **Close Position**
```rust
close_trade(env, trader_address);
```

4. **Liquidate Position**
```rust
liquidate_position(env, liquidator_address, undercollateralized_user);
```

## Testing
Run tests with:
```bash
cargo test
```
See `test.rs` for detailed test cases covering position opening/closing, fee calculation, and liquidation scenarios.

## Dependencies
- `soroban-sdk`: Soroban Smart Contract SDK
- `sep_41_token`: Standard token interface implementation

## License
MIT License