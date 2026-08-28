//! Per-address ticket cap tests salvaged from repository root (#846).

use crate::{Error, RaffleConfig, RaffleInstance, RaffleInstanceClient};
use raffle_shared::RandomnessSource;
use soroban_sdk::{
    testutils::Address as _,
    token::StellarAssetClient,
    Address, BytesN, Env, String, Vec,
};

fn setup_token(env: &Env) -> (Address, StellarAssetClient<'_>) {
    let admin = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    (token, StellarAssetClient::new(env, &token))
}

#[test]
fn per_address_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer = Address::generate(&env);
    let (token, token_client) = setup_token(&env);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Per-address cap test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        max_tickets_per_address: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1_000_000,
        payment_token: token.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10_000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: Vec::new(&env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    token_client.mint(&buyer, &20_000_000);
    token_client.mint(&creator, &10_000_000);
    client.deposit_prize();

    assert!(client.try_buy_tickets(&buyer, &3).is_ok());
    assert!(client.try_buy_tickets(&buyer, &2).is_ok());
    assert_eq!(
        client.try_buy_tickets(&buyer, &1),
        Err(Ok(Error::ExceedsMaxTicketsPerAddress))
    );
}

#[test]
fn per_address_cap_zero_means_unlimited() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer = Address::generate(&env);
    let (token, token_client) = setup_token(&env);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Unlimited per address"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1_000_000,
        payment_token: token.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10_000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: Vec::new(&env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    token_client.mint(&buyer, &20_000_000);
    token_client.mint(&creator, &10_000_000);
    client.deposit_prize();

    assert!(client.try_buy_tickets(&buyer, &5).is_ok());
    assert_eq!(client.try_buy_tickets(&buyer, &5).unwrap(), 10);
}

#[test]
fn per_address_cap_multiple_buyers() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let (token, token_client) = setup_token(&env);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

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
        payment_token: token.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10_000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: Vec::new(&env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    token_client.mint(&buyer1, &10_000_000);
    token_client.mint(&buyer2, &10_000_000);
    token_client.mint(&creator, &10_000_000);
    client.deposit_prize();

    client.buy_tickets(&buyer1, &3);
    client.buy_tickets(&buyer2, &3);

    assert_eq!(
        client.try_buy_tickets(&buyer1, &1),
        Err(Ok(Error::ExceedsMaxTicketsPerAddress))
    );
    assert_eq!(
        client.try_buy_tickets(&buyer2, &1),
        Err(Ok(Error::ExceedsMaxTicketsPerAddress))
    );
}
