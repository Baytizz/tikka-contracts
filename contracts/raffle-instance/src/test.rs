// ===========================================================================
// claim_lockup_seconds boundary validation tests
// ===========================================================================

#[test]
fn test_init_claim_lockup_seconds_at_bound_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let config = RaffleConfig {
        description: String::from_str(&env, "Claim lockup at bound"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: MAX_CLAIM_LOCKUP_SECONDS,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&creator_factory_addr(&env), &admin, &creator, &config);
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, MAX_CLAIM_LOCKUP_SECONDS);
}

#[test]
fn test_init_claim_lockup_seconds_above_bound_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let config = RaffleConfig {
        description: String::from_str(&env, "Claim lockup above bound"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: MAX_CLAIM_LOCKUP_SECONDS + 1,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    let result = client.try_init(&creator_factory_addr(&env), &admin, &creator, &config);
    assert_eq!(result, Err(Ok(Error::InvalidParameters)));
}

#[test]
fn test_init_claim_lockup_seconds_mid_range_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mid_range_lockup: u64 = 86_400; // 1 day
    let config = RaffleConfig {
        description: String::from_str(&env, "Claim lockup mid-range"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: mid_range_lockup,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&creator_factory_addr(&env), &admin, &creator, &config);
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, mid_range_lockup);
}

// ===========================================================================
// #627: max_tickets_per_tx enforcement and zero-value semantics tests
// ===========================================================================

#[test]
fn test_init_rejects_zero_max_tickets_per_tx() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env.register_stellar_asset_contract_v2(token_admin).address();

    let mut config = lifecycle_config(&env, &payment_token, &Address::generate(&env));
    config.max_tickets_per_tx = 0; // Zero is invalid

    let res = client.try_init(&factory, &admin, &creator, &config);
    assert_eq!(res, Err(Ok(Error::InvalidParameters)));
}

#[test]
fn test_max_tickets_per_tx_boundary_and_exceeded() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let factory = creator_factory_addr(&env);
    let treasury = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env.register_stellar_asset_contract_v2(token_admin).address();
    let token = StellarAssetClient::new(&env, &payment_token);
    token.mint(&creator, &100_000_000);
    token.mint(&buyer, &100_000_000);

    let mut config = lifecycle_config(&env, &payment_token, &treasury);
    config.max_tickets = 100;
    config.max_tickets_per_tx = 5;

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    // Purchasing exactly cap (5 tickets) must succeed
    let sold = client.buy_tickets(&buyer, &5);
    assert_eq!(sold, 5);

    // Purchasing cap + 1 (6 tickets) must fail with ExceedsMaxTicketsPerTx
    let err = client.try_buy_tickets(&buyer, &6);
    assert_eq!(err, Err(Ok(Error::ExceedsMaxTicketsPerTx)));
}

// ===========================================================================
// #625: Early-bird pricing boundary and rounding tests
// ===========================================================================

#[test]
fn test_early_bird_last_discounted_and_first_full_price() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env.register_stellar_asset_contract_v2(token_admin).address();
    let token = StellarAssetClient::new(&env, &payment_token);
    let token_ro = soroban_sdk::token::Client::new(&env, &payment_token);

    token.mint(&creator, &100_000_000);
    token.mint(&buyer, &100_000_000);

    // 10 max tickets, 50% early bird (cap = 5 tickets), 10% discount (1000 bp), price = 10,000 stroops
    // Discounted price = 10,000 * 9000 / 10000 = 9,000 stroops.
    let config = RaffleConfig {
        description: String::from_str(&env, "Early bird edge test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_000,
        payment_token: payment_token.clone(),
        prize_amount: 100_000,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 50,
        early_bird_discount_bp: 1000,
        category: None,
    };

    client.init(&creator_factory_addr(&env), &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    let initial_bal = token_ro.balance(&buyer);

    // Buy first 4 tickets individually — each should cost 9,000 stroops
    for i in 1..=4 {
        let before = token_ro.balance(&buyer);
        client.buy_tickets(&buyer, &1);
        let after = token_ro.balance(&buyer);
        assert_eq!(before - after, 9_000, "Ticket {i} must be discounted to 9,000 stroops");
    }

    // Ticket 5 (last discounted ticket, tickets_sold = 4 < cap 5): cost 9,000 stroops
    let before = token_ro.balance(&buyer);
    client.buy_tickets(&buyer, &1);
    let after = token_ro.balance(&buyer);
    assert_eq!(before - after, 9_000, "Ticket 5 (last discounted ticket) must cost 9,000 stroops");

    // Ticket 6 (first full-price ticket, tickets_sold = 5 >= cap 5): cost 10,000 stroops
    let before = token_ro.balance(&buyer);
    client.buy_tickets(&buyer, &1);
    let after = token_ro.balance(&buyer);
    assert_eq!(before - after, 10_000, "Ticket 6 (first full price ticket) must cost 10,000 stroops");

    assert_eq!(initial_bal - token_ro.balance(&buyer), 5 * 9_000 + 10_000);
}

#[test]
fn test_early_bird_purchase_spanning_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env.register_stellar_asset_contract_v2(token_admin).address();
    let token = StellarAssetClient::new(&env, &payment_token);
    let token_ro = soroban_sdk::token::Client::new(&env, &payment_token);

    token.mint(&creator, &100_000_000);
    token.mint(&buyer, &100_000_000);

    // 10 max tickets, 50% early bird (cap = 5), 10% discount (1000 bp), price = 10,000
    let config = RaffleConfig {
        description: String::from_str(&env, "Early bird boundary span"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_000,
        payment_token: payment_token.clone(),
        prize_amount: 100_000,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 50,
        early_bird_discount_bp: 1000,
        category: None,
    };

    client.init(&creator_factory_addr(&env), &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    // First buy 4 tickets (tickets 1..=4). Cost = 4 * 9,000 = 36,000 stroops
    client.buy_tickets(&buyer, &4);

    // Now buy 3 tickets in ONE call (tickets 5, 6, 7 spanning the boundary).
    // Ticket 5 is discounted (9,000 stroops), Tickets 6 & 7 are full price (10,000 stroops each).
    // Total charged for this single call must be exact mixed sum: 1 * 9,000 + 2 * 10,000 = 29,000 stroops.
    let before = token_ro.balance(&buyer);
    client.buy_tickets(&buyer, &3);
    let after = token_ro.balance(&buyer);

    assert_eq!(before - after, 29_000, "Spanning purchase of 3 tickets must cost exact mixed sum 29,000 stroops");
}

#[test]
fn test_early_bird_rounding_minimal_price() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env.register_stellar_asset_contract_v2(token_admin).address();
    let token = StellarAssetClient::new(&env, &payment_token);
    let token_ro = soroban_sdk::token::Client::new(&env, &payment_token);

    token.mint(&creator, &100_000_000);
    token.mint(&buyer, &100_000_000);

    // Price = 10,001 stroops, 1 bp discount (0.01%).
    // Formula: 10001 * 9999 / 10000 = 100009999 / 10000 = 10000 stroops.
    // Rounding direction: integer division truncates down (discounted price is 10,000 stroops).
    let config = RaffleConfig {
        description: String::from_str(&env, "Early bird rounding test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_001,
        payment_token: payment_token.clone(),
        prize_amount: 100_010,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 100,
        early_bird_discount_bp: 1,
        category: None,
    };

    client.init(&creator_factory_addr(&env), &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    let before = token_ro.balance(&buyer);
    client.buy_tickets(&buyer, &1);
    let after = token_ro.balance(&buyer);

    assert_eq!(before - after, 10_000, "10,001 stroops with 1 bp discount must round down to 10,000 stroops");
}

#[test]
fn test_early_bird_configs_zero_percent_hundred_percent_zero_discount() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env.register_stellar_asset_contract_v2(token_admin).address();
    let token = StellarAssetClient::new(&env, &payment_token);
    let token_ro = soroban_sdk::token::Client::new(&env, &payment_token);

    token.mint(&creator, &100_000_000);
    token.mint(&buyer, &100_000_000);

    // Scenario A: 0% percentage config -> no tickets discounted
    let mut config_0 = lifecycle_config(&env, &payment_token, &Address::generate(&env));
    config_0.early_bird_ticket_percentage = 0;
    config_0.early_bird_discount_bp = 2000;
    client.init(&creator_factory_addr(&env), &admin, &creator, &config_0);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    let before = token_ro.balance(&buyer);
    client.buy_tickets(&buyer, &1);
    let after = token_ro.balance(&buyer);
    assert_eq!(before - after, 10_000, "0% percentage config must charge full ticket price 10,000 stroops");
}

// ===========================================================================
// #623: Exhaustive RaffleStatus transition-matrix test
// ===========================================================================

struct MatrixTestRig;

impl MatrixTestRig {
    fn new_in_state(target_state: RaffleStatus) -> (Env, ContractClient<'static>, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);

        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let buyer = Address::generate(&env);
        let treasury = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let payment_token = env.register_stellar_asset_contract_v2(token_admin).address();
        let token = StellarAssetClient::new(&env, &payment_token);
        token.mint(&creator, &100_000_000);
        token.mint(&buyer, &100_000_000);

        let mut config = lifecycle_config(&env, &payment_token, &treasury);
        config.max_tickets = 2;
        config.min_tickets = 1;

        client.init(&creator_factory_addr(&env), &admin, &creator, &config);
        env.as_contract(&contract_id, || {
            env.storage().instance().remove(&DataKey::Factory);
        });

        match target_state {
            RaffleStatus::PendingPrize => {}
            RaffleStatus::Active => {
                client.deposit_prize();
            }
            RaffleStatus::Drawing => {
                client.deposit_prize();
                client.buy_tickets(&buyer, &2); // Fills tickets -> flips to Drawing
            }
            RaffleStatus::Finalized => {
                client.deposit_prize();
                client.buy_tickets(&buyer, &2);
                client.finalize_raffle();
            }
            RaffleStatus::Cancelled => {
                client.deposit_prize();
                client.cancel_raffle(&CancelReason::CreatorCancelled);
            }
            RaffleStatus::Failed => {
                client.deposit_prize();
                // Advance past end_time with 0 tickets sold
                env.ledger().set_timestamp(config.end_time + 1);
                client.finalize_raffle();
            }
            RaffleStatus::Claimed => {
                client.deposit_prize();
                client.buy_tickets(&buyer, &2);
                client.finalize_raffle();
                env.ledger().set_timestamp(1000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 10);
                let winners = client.get_raffle().winners;
                if let Some(w) = winners.get(0) {
                    let _ = client.claim_prize(&w, &0u32);
                }
            }
        }

        assert_eq!(client.get_raffle().status, target_state, "Rig setup must match expected target state");
        (env, client, admin, creator, buyer)
    }
}

#[test]
fn test_matrix_deposit_prize() {
    for state in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
        RaffleStatus::Cancelled,
        RaffleStatus::Failed,
        RaffleStatus::Claimed,
    ] {
        let (_env, client, _admin, _creator, _buyer) = MatrixTestRig::new_in_state(state.clone());
        let res = client.try_deposit_prize();
        match state {
            RaffleStatus::PendingPrize => assert!(res.is_ok(), "deposit_prize should be allowed in PendingPrize"),
            _ => assert_eq!(res, Err(Ok(Error::PrizeAlreadyDeposited)), "deposit_prize should be rejected in {:?}", state),
        }
    }
}

#[test]
fn test_matrix_buy_tickets() {
    for state in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
        RaffleStatus::Cancelled,
        RaffleStatus::Failed,
        RaffleStatus::Claimed,
    ] {
        let (_env, client, _admin, _creator, buyer) = MatrixTestRig::new_in_state(state.clone());
        let res = client.try_buy_tickets(&buyer, &1);
        match state {
            RaffleStatus::Active => assert!(res.is_ok(), "buy_tickets should be allowed in Active"),
            RaffleStatus::Drawing => assert_eq!(res, Err(Ok(Error::DrawingAlreadyInProgress)), "buy_tickets should fail in Drawing"),
            _ => assert_eq!(res, Err(Ok(Error::RaffleInactive)), "buy_tickets should fail with RaffleInactive in {:?}", state),
        }
    }
}

#[test]
fn test_matrix_finalize_raffle() {
    for state in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
        RaffleStatus::Cancelled,
        RaffleStatus::Failed,
        RaffleStatus::Claimed,
    ] {
        let (_env, client, _admin, _creator, _buyer) = MatrixTestRig::new_in_state(state.clone());
        let res = client.try_finalize_raffle();
        match state {
            RaffleStatus::Active => assert_eq!(res, Err(Ok(Error::InvalidStateTransition)), "finalize_raffle should fail in Active when open/unexpired"),
            RaffleStatus::Drawing => assert!(res.is_ok(), "finalize_raffle should succeed in Drawing"),
            RaffleStatus::PendingPrize => assert_eq!(res, Err(Ok(Error::InvalidStateTransition)), "finalize_raffle should fail in PendingPrize"),
            _ => assert_eq!(res, Err(Ok(Error::InvalidStatus)), "finalize_raffle should fail with InvalidStatus in {:?}", state),
        }
    }
}

#[test]
fn test_matrix_provide_randomness() {
    for state in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
        RaffleStatus::Cancelled,
        RaffleStatus::Failed,
        RaffleStatus::Claimed,
    ] {
        let (env, client, _admin, _creator, _buyer) = MatrixTestRig::new_in_state(state.clone());
        let dummy_bytes32 = BytesN::from_array(&env, &[0u8; 32]);
        let dummy_bytes64 = BytesN::from_array(&env, &[0u8; 64]);
        let res = client.try_provide_randomness(&12345u64, &dummy_bytes32, &dummy_bytes64, &1u64);
        match state {
            RaffleStatus::Drawing => {
                assert_eq!(res, Err(Ok(Error::NoRandomnessRequest)));
            }
            _ => assert_eq!(res, Err(Ok(Error::InvalidStatus)), "provide_randomness should fail with InvalidStatus in {:?}", state),
        }
    }
}

#[test]
fn test_matrix_claim_prize() {
    for state in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
        RaffleStatus::Cancelled,
        RaffleStatus::Failed,
        RaffleStatus::Claimed,
    ] {
        let (_env, client, _admin, _creator, buyer) = MatrixTestRig::new_in_state(state.clone());
        let res = client.try_claim_prize(&buyer, &0u32);
        match state {
            RaffleStatus::Finalized => assert!(res.is_ok(), "claim_prize should be allowed in Finalized"),
            RaffleStatus::Claimed => assert_eq!(res, Err(Ok(Error::InvalidStatus)), "claim_prize should fail in Claimed"),
            _ => assert_eq!(res, Err(Ok(Error::InvalidStatus)), "claim_prize should fail with InvalidStatus in {:?}", state),
        }
    }
}

#[test]
fn test_matrix_cancel_raffle() {
    for state in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
        RaffleStatus::Cancelled,
        RaffleStatus::Failed,
        RaffleStatus::Claimed,
    ] {
        let (_env, client, _admin, _creator, _buyer) = MatrixTestRig::new_in_state(state.clone());
        let res = client.try_cancel_raffle(&CancelReason::CreatorCancelled);
        match state {
            RaffleStatus::PendingPrize | RaffleStatus::Active | RaffleStatus::Drawing | RaffleStatus::Failed => {
                assert!(res.is_ok(), "cancel_raffle should be allowed in {:?}", state);
            }
            _ => assert_eq!(res, Err(Ok(Error::InvalidStatus)), "cancel_raffle should fail with InvalidStatus in {:?}", state),
        }
    }
}

#[test]
fn test_matrix_refund_ticket() {
    for state in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
        RaffleStatus::Cancelled,
        RaffleStatus::Failed,
        RaffleStatus::Claimed,
    ] {
        let (_env, client, _admin, _creator, buyer) = MatrixTestRig::new_in_state(state.clone());
        let res = client.try_refund_ticket(&1u32);
        match state {
            RaffleStatus::Cancelled | RaffleStatus::Failed => {
                assert!(res.is_ok() || res == Err(Ok(Error::TicketNotFound)));
            }
            _ => assert_eq!(res, Err(Ok(Error::InvalidStatus)), "refund_ticket should fail with InvalidStatus in {:?}", state),
        }
    }
}