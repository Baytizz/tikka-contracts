// Test for per-address ticket cap feature
// Add this to contracts/raffle-instance/src/test.rs

#[test]
fn test_per_address_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    // Create raffle with max 5 tickets per address
    let config = RaffleConfig {
        description: String::from_str(&env, "Per-address cap test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        max_tickets_per_address: 5, // Cap at 5 per address
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1_000_000,
        payment_token: token.address.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    
    // Mint tokens for buyer
    token.mint(&buyer, &20_000_000);
    
    // Deposit prize
    token.mint(&creator, &10_000_000);
    client.deposit_prize();

    // Buy 3 tickets - should succeed
    let result = client.try_buy_tickets(&buyer, &3);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 3);

    // Buy 2 more tickets - should succeed (total = 5, at cap)
    let result = client.try_buy_tickets(&buyer, &2);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 5);

    // Try to buy 1 more ticket - should fail with ExceedsMaxTicketsPerAddress
    let result = client.try_buy_tickets(&buyer, &1);
    assert_eq!(result, Err(Ok(Error::ExceedsMaxTicketsPerAddress)));
}

#[test]
fn test_per_address_cap_zero_unlimited() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    // Create raffle with max_tickets_per_address = 0 (unlimited)
    let config = RaffleConfig {
        description: String::from_str(&env, "Unlimited per address"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        max_tickets_per_address: 0, // 0 means unlimited
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1_000_000,
        payment_token: token.address.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    
    // Mint tokens for buyer
    token.mint(&buyer, &20_000_000);
    
    // Deposit prize
    token.mint(&creator, &10_000_000);
    client.deposit_prize();

    // Buy all 10 tickets in two transactions - should succeed
    let result = client.try_buy_tickets(&buyer, &5);
    assert!(result.is_ok());
    
    let result = client.try_buy_tickets(&buyer, &5);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 10);
}

#[test]
fn test_per_address_cap_multiple_buyers() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    // Create raffle with max 3 tickets per address
    let config = RaffleConfig {
        description: String::from_str(&env, "Multiple buyers test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        max_tickets_per_address: 3,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1_000_000,
        payment_token: token.address.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    
    // Mint tokens
    token.mint(&buyer1, &10_000_000);
    token.mint(&buyer2, &10_000_000);
    token.mint(&creator, &10_000_000);
    
    client.deposit_prize();

    // Each buyer can buy up to 3 tickets
    client.buy_tickets(&buyer1, &3);
    client.buy_tickets(&buyer2, &3);
    
    // Both buyers hit their cap
    let result = client.try_buy_tickets(&buyer1, &1);
    assert_eq!(result, Err(Ok(Error::ExceedsMaxTicketsPerAddress)));
    
    let result = client.try_buy_tickets(&buyer2, &1);
    assert_eq!(result, Err(Ok(Error::ExceedsMaxTicketsPerAddress)));
}
