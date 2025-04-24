#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _}, Address, Env, Vec, Map, IntoVal};
use sep_41_token::testutils::{MockTokenClient, MockTokenWASM};

// Test helper to create a token mock that simulates the pUSD token
fn create_token_contract(e: &Env) -> (Address, MockTokenClient) {
    let admin = Address::generate(&e);
    let token_id = e.register_contract_wasm(None, MockTokenWASM);
    let token_client = MockTokenClient::new(&e, &token_id);
    token_client.initialize(
        &admin,
        &7,
        &String::from_str(&e, "Name"),
        &String::from_str(&e, "Symbol"),
    );
    (token_id, token_client)
}


// Test helper to setup test environment with initialized contract
fn setup<'a>(e: &'a Env) -> (Address, PerpContractClient<'a>, Address, MockTokenClient<'a>) {
    let client_id = e.register_contract(None, PerpContract);
    let client = PerpContractClient::new(e, &client_id);
    let oracle = Address::generate(e);
    
    let (token_id, token) = create_token_contract(e);
    
    // Initialize the contract
    client.initialize(
        &"BTC".into_val(e),
        &10_i128,  // 10x leverage
        &token_id,
        &oracle
    );
    
    // Set a mock price
    e.as_contract(&client_id, || {
        e.storage().instance().set(&PRICE, &50000_i128);
    });
    
    (client_id, client, token_id, token)
}

// Helper to mint tokens for test users
fn mint_tokens(env: &Env, token_id: &Address, user: &Address, amount: i128) {
    let mock_token = MockTokenClient::new(env, token_id);
    env.mock_all_auths();
    mock_token.mint(user, &amount);
}

#[test]
fn test_initialize() {
    let env = Env::default();
    let (_client_id, client, token_id, _) = setup(&env);
    
    // Check stored values
    env.as_contract(&client.address, || {
        let asset: String = env.storage().instance().get(&ASSET).unwrap();
        let leverage: i128 = env.storage().instance().get(&LEVERAGE).unwrap();
        let p_usd: Address = env.storage().instance().get(&PUSD).unwrap();
        let margin_req: i128 = env.storage().instance().get(&MARGIN_REQ).unwrap();
        let long_pos: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        let short_pos: i128 = env.storage().instance().get(&SHORT_POS).unwrap();
        
        assert_eq!(asset, String::from_str(&env, "BTC"));
        assert_eq!(leverage, 10_i128);
        assert_eq!(p_usd, token_id);
        assert_eq!(margin_req, 300_i128);
        assert_eq!(long_pos, 0_i128);
        assert_eq!(short_pos, 0_i128);
    });
}

#[test]
fn test_calculate_position_empty() {
    // Set up environment and contract
    let env = Env::default();
    let (_, client, _, _) = setup(&env);

    // Use a random user address
    let user = Address::generate(&env);
    // No position => should return zero
    let result = client.calculate_position(&user);
    assert_eq!(result, 0_i128);
}

#[test]
fn test_place_trade_long() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and mint them some tokens
    let trader = Address::generate(&env);
    mint_tokens(&env, &token_id, &trader, 1000_i128);

    // Authorize the trader
    env.mock_all_auths();

    // Approve spend
    token.approve(&trader, &client_id, &1000_i128, &0_u32);
    // Place a long trade
    client.place_trade(&trader, &1000_i128, &true);
    
    // Verify position was created and long position increased
    env.as_contract(&client.address, || {
        let positions: Map<Address, Position> = env.storage().persistent().get(&POSITIONS).unwrap();
        let position = positions.get(trader.clone()).unwrap();
        
        assert_eq!(position.value, 1000_i128); // No fee in this simple case
        assert_eq!(position.open_price, 50000_i128);
        assert_eq!(position.close_price, 0_i128);
        assert_eq!(position.long, true);
        
        let total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        assert_eq!(total_long, 1000_i128);
    });
}


#[test]
fn test_place_trade_short() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and mint them some tokens
    let trader = Address::generate(&env);
    mint_tokens(&env, &token_id, &trader, 500_i128);
    
    // Authorize the trader
    env.mock_all_auths();
    
    // Approve spend
    token.approve(&trader, &client_id, &500_i128, &0_u32);
    // Place a short trade
    client.place_trade(&trader, &500_i128, &false);
    
    // Verify position was created and short position increased
    env.as_contract(&client.address, || {
        let positions: Map<Address, Position> = env.storage().persistent().get(&POSITIONS).unwrap();
        let position = positions.get(trader.clone()).unwrap();
        
        assert_eq!(position.value, 500_i128);
        assert_eq!(position.open_price, 50000_i128);
        assert_eq!(position.close_price, 0_i128);
        assert_eq!(position.long, false);
        
        let total_short: i128 = env.storage().instance().get(&SHORT_POS).unwrap();
        assert_eq!(total_short, 500_i128);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_place_trade_zero_value() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    
    // Create a user
    let trader = Address::generate(&env);
    
    // Authorize the trader
    env.mock_all_auths();
    
    // Try to place a trade with zero value
    client.place_trade(&trader, &0_i128, &true);
    // Expected to panic with ContractError::ZeroValue
}


#[test]
fn test_calculate_position_long_profit() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and place a long position
    let trader = Address::generate(&env);
    mint_tokens(&env, &token_id, &trader, 1000_i128);
    
    env.mock_all_auths();

    token.approve(&trader, &client_id, &1000_i128, &0_u32);
    client.place_trade(&trader, &1000_i128, &true);
    
    // Price goes up
    env.as_contract(&client_id, || {
        env.storage().instance().set(&PRICE, &55000_i128);
    });
    
    // Calculate position - should show profit
    let position_value = client.calculate_position(&trader);
    
    // Expected profit calculation:
    // Price increase: 55000 - 50000 = 5000
    // Leverage: 10x
    // Position size: 1000
    // Multiplier: 10 * 1000 / 50000 = 0.2
    // Profit: 5000 * 0.2 = 1000
    // Total value: 1000 (initial) + 1000 (profit) = 2000
    assert_eq!(position_value, 2000_i128);
}


#[test]
fn test_calculate_position_long_loss() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and place a long position
    let trader = Address::generate(&env);
    mint_tokens(&env, &token_id, &trader, 1000_i128);
    
    env.mock_all_auths();
    token.approve(&trader, &client_id, &1000_i128, &0_u32);
    client.place_trade(&trader, &1000_i128, &true);
    
    // Price goes down
    env.as_contract(&client_id, || {
        env.storage().instance().set(&PRICE, &45000_i128);
    });
    
    // Calculate position - should show loss
    let position_value = client.calculate_position(&trader);
    
    // Expected loss calculation:
    // Price decrease: 50000 - 45000 = 5000
    // Multiplier: 10 * 1000 / 50000 = 0.2
    // Loss: 5000 * 0.2 = 1000
    // Since loss equals margin, position would be worth 0
    assert_eq!(position_value, 0_i128);
}


#[test]
fn test_calculate_position_short_profit() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and place a short position
    let trader = Address::generate(&env);
    mint_tokens(&env, &token_id, &trader, 1000_i128);
    
    env.mock_all_auths();
    token.approve(&trader, &client_id, &1000_i128, &0_u32);
    client.place_trade(&trader, &1000_i128, &false);
    
    // Price goes down (profit for short)
    env.as_contract(&client_id, || {
        env.storage().instance().set(&PRICE, &45000_i128);
    });
    
    // Calculate position - should show profit
    let position_value = client.calculate_position(&trader);
    
    // Expected profit calculation:
    // Price decrease: 50000 - 45000 = 5000
    // Multiplier: 10 * 1000 / 50000 = 0.2
    // Profit: 5000 * 0.2 = 1000
    // Total value: 1000 (initial) + 1000 (profit) = 2000
    assert_eq!(position_value, 2000_i128);
}


#[test]
fn test_calculate_fee() {
    let env = Env::default();
    let (client_id, client, _, _) = setup(&env);
    
    // Set up existing positions for fee calculation test
    env.as_contract(&client_id, || {
        env.storage().instance().set(&LONG_POS, &5000_i128);
        env.storage().instance().set(&SHORT_POS, &2000_i128);
    });
    

    // Calculate fee for a trade that increases imbalance
    let fee_for_long = client.calculate_fee(&1000_i128, &true);

    // Should have some fee as it increases imbalance
    assert!(fee_for_long > 0);

    // Calculate fee for a trade that reduces imbalance
    let fee_for_short = client.calculate_fee(&1000_i128, &false);
    
    // Should have zero fee as it reduces imbalance
    assert_eq!(fee_for_short, 0);

}


#[test]
fn test_close_trade() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and place a position
    let trader = Address::generate(&env);
    mint_tokens(&env, &token_id, &trader, 1000_i128);
    
    env.mock_all_auths();
    token.approve(&trader, &client_id, &1000_i128, &0_u32);
    client.place_trade(&trader, &1000_i128, &true);
    
    // Price goes up
    env.as_contract(&client_id, || {
        env.storage().instance().set(&PRICE, &55000_i128);
    });
    
    // Close the trade
    client.close_trade(&trader);
    
    // Check the trade history and that position was removed
    env.as_contract(&client.address, || {
        let positions: Map<Address, Position> = env.storage().persistent().get(&POSITIONS).unwrap();
        assert!(!positions.contains_key(trader.clone()));
        
        let history: Vec<Position> = env.storage().instance().get(&TRADE_HISTORY).unwrap();
        assert_eq!(history.len(), 1);
        
        let closed_position = history.get_unchecked(0);
        assert_eq!(closed_position.value, 1000_i128);
        assert_eq!(closed_position.open_price, 50000_i128);
        assert_eq!(closed_position.close_price, 55000_i128);
        assert_eq!(closed_position.long, true);
        
        let total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        assert_eq!(total_long, 0_i128);
    });
}


#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_close_nonexistent_trade() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    
    // Create a user but don't place any trades
    let trader = Address::generate(&env);
    
    env.mock_all_auths();
    // Try to close a non-existent position
    client.close_trade(&trader);
    // Expected to panic with ContractError::PositionNotOpen
}



#[test]
fn test_liquidate_position() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and a liquidator
    let trader = Address::generate(&env);
    let liquidator = Address::generate(&env);
    
    mint_tokens(&env, &token_id, &trader, 1000_i128);
    
    env.mock_all_auths();
    token.approve(&trader, &client_id, &1000_i128, &0_u32);
    client.place_trade(&trader, &1000_i128, &true);
    
    // Price drops significantly - position now undercollateralized
    env.as_contract(&client_id, || {
        env.storage().instance().set(&PRICE, &100_i128);
    });
    
    // Liquidate the position
    client.liquidate_position(&liquidator, &trader);
    
    // Check that position was removed and liquidator received reward
    env.as_contract(&client.address, || {
        let positions: Map<Address, Position> = env.storage().persistent().get(&POSITIONS).unwrap();
        assert!(!positions.contains_key(trader.clone()));
        
        let history: Vec<Position> = env.storage().instance().get(&TRADE_HISTORY).unwrap();
        assert_eq!(history.len(), 1);
        
        let closed_position = history.get_unchecked(0);
        assert_eq!(closed_position.close_price, 100_i128);
        
        let total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        assert_eq!(total_long, 0_i128);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_liquidate_healthy_position() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create a user and a liquidator
    let trader = Address::generate(&env);
    let liquidator = Address::generate(&env);
    
    mint_tokens(&env, &token_id, &trader, 1000_i128);
    
    env.mock_all_auths();
    token.approve(&trader, &client_id, &1000_i128, &0_u32);
    client.place_trade(&trader, &1000_i128, &true);
    
    // Price drops but position still healthy
    env.as_contract(&client_id, || {
        env.storage().instance().set(&PRICE, &49000_i128);
        // Normal margin requirement
        env.storage().instance().set(&MARGIN_REQ, &300_i128);
    });
    
    // Try to liquidate the position
    client.liquidate_position(&liquidator, &trader);
    // Expected to panic with ContractError::AboveMargin
}



#[test]
fn test_calculate_fee_balanced_market() {
    let env = Env::default();
    let (client_id, _, _, _) = setup(&env);
    
    // Set up balanced market
    env.as_contract(&client_id, || {
        env.storage().instance().set(&LONG_POS, &5000_i128);
        env.storage().instance().set(&SHORT_POS, &5000_i128);
        
        // Fee should be zero for both sides in balanced market
        let fee_long = PerpContract::calculate_fee(&env, 1000_i128, true);
        let fee_short = PerpContract::calculate_fee(&env, 1000_i128, false);
        
        assert_eq!(fee_long, 0);
        assert_eq!(fee_short, 0);
    });
}

#[test]
fn test_multiple_positions() {
    let env = Env::default();
    let (client_id, client, token_id, token) = setup(&env);
    
    // Create multiple users
    let trader1 = Address::generate(&env);
    let trader2 = Address::generate(&env);
    
    mint_tokens(&env, &token_id, &trader1, 1000_i128);
    mint_tokens(&env, &token_id, &trader2, 2000_i128);
    
    env.mock_all_auths();
    
    // Place different positions
    token.approve(&trader1, &client_id, &1000_i128, &0_u32);
    client.place_trade(&trader1, &1000_i128, &true);  // Long

    token.approve(&trader2, &client_id, &2000_i128, &0_u32);
    client.place_trade(&trader2, &2000_i128, &false); // Short
    
    // Verify positions were created correctly
    env.as_contract(&client.address, || {
        let positions: Map<Address, Position> = env.storage().persistent().get(&POSITIONS).unwrap();
        
        let position1 = positions.get(trader1.clone()).unwrap();
        let position2 = positions.get(trader2.clone()).unwrap();
        
        assert_eq!(position1.value, 1000_i128);
        assert_eq!(position1.long, true);
        
        assert_eq!(position2.value, 2000_i128);
        assert_eq!(position2.long, false);
        
        let total_long: i128 = env.storage().instance().get(&LONG_POS).unwrap();
        let total_short: i128 = env.storage().instance().get(&SHORT_POS).unwrap();
        
        assert_eq!(total_long, 1000_i128);
        assert_eq!(total_short, 2000_i128);
    });
}