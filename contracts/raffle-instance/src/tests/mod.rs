//! Shared fixtures, helpers, and mock contracts for raffle-instance integration tests.

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use raffle_shared::{DEFAULT_CLAIM_LOCKUP_SECONDS, DEFAULT_SWAP_DEADLINE_SECONDS};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token::{self, StellarAssetClient},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, String, IntoVal, Val, Symbol,
};

#[contract]
pub struct MockFactory;

#[contractimpl]
impl MockFactory {
    pub fn record_volume(_env: Env, _token: Address, _amount: i128) {}
    pub fn track_participant(_env: Env, _participant: Address) {}
}

pub type Contract = RaffleInstance;
pub type ContractClient<'a> = RaffleInstanceClient<'a>;

pub fn assert_event<T: IntoVal<Env, Val>>(
    env: &Env,
    expected_contract: &Address,
    expected_topic: &str,
    expected_payload: T,
) {
    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(&last.0, expected_contract);
    assert_eq!(last.1.get(0).unwrap(), Symbol::new(env, "tikka").into_val(env));
    assert_eq!(last.1.get(1).unwrap(), Symbol::new(env, expected_topic).into_val(env));
    assert_eq!(last.2, expected_payload.into_val(env));
}

pub(crate) fn assert_drawing_lock_cleared(env: &Env, contract_id: &Address) {
    let is_set: bool = env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::DrawingLock)
            .unwrap_or(false)
    });
    assert!(!is_set, "DrawingLock must be cleared");
}

pub fn create_token<'a>(env: &'a Env, admin: &Address) -> (Address, StellarAssetClient<'a>) {
    let payment_token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    (
        payment_token.clone(),
        StellarAssetClient::new(env, &payment_token),
    )
}

pub fn setup_scale_raffle(
    env: &Env,
    max_tickets: u32,
    max_tickets_per_tx: u32,
    prize_amount: i128,
) -> (
    ContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'_>,
) {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    let token_admin = Address::generate(env);
    let (payment_token, token_mint) = create_token(env, &token_admin);
    token_mint.mint(&creator, &prize_amount * 2);
    token_mint.mint(&buyer, &prize_amount * 2);

    let config = RaffleConfig {
        description: String::from_str(env, "scale benchmark"),
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[88u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    (
        client,
        contract_id,
        creator,
        buyer,
        payment_token,
        token_mint,
    )
}

pub fn record_costs<F: FnOnce()>(env: &Env, f: F) -> (u64, u64) {
    env.cost_estimate().budget().reset_default();
    f();
    let budget = env.cost_estimate().budget();
    (budget.cpu_instruction_cost(), budget.memory_bytes_cost())
}

pub const BUY_TICKETS_1K_CPU_CEILING: u64 = 30_000_000;
pub const BUY_TICKETS_1K_MEM_CEILING: u64 = 10 * 1024 * 1024;
pub const FINALIZE_10K_CPU_CEILING: u64 = 80_000_000;
pub const FINALIZE_10K_MEM_CEILING: u64 = 24 * 1024 * 1024;
pub const GET_MY_TICKETS_10K_CPU_CEILING: u64 = 15_000_000;
pub const GET_MY_TICKETS_10K_MEM_CEILING: u64 = 8 * 1024 * 1024;

pub fn setup_active_raffle(
    env: &Env,
) -> (ContractClient<'_>, Address, Address, Address, Address) {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    let token_admin = Address::generate(env);
    let (token_addr, token_mint) = create_token(env, &token_admin);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer, &1_000_000);

    let config = RaffleConfig {
        description: String::from_str(env, "active raffle"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr,
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[9u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer, &1);

    (client, contract_id, admin, creator, buyer)
}

pub fn lifecycle_config(env: &Env, payment_token: &Address, treasury: &Address) -> RaffleConfig {
    RaffleConfig {
        description: String::from_str(env, "lifecycle"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 100,
        treasury_address: Some(treasury.clone()),
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[11u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 10,
        early_bird_discount_bp: 500,
    }
}

pub fn init_bounds_config(
    env: &Env,
    payment_token: &Address,
    metadata_byte: u8,
) -> RaffleConfig {
    RaffleConfig {
        description: String::from_str(env, "bounds"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[metadata_byte; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
    }
}

pub fn assert_metadata_hash(client: &ContractClient<'_>, expected: &BytesN<32>) {
    let raffle = client.get_raffle();
    assert_eq!(raffle.metadata_hash, *expected);
}

pub fn init_bounds_env() -> (Env, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    (env, factory, admin, creator, payment_token, token_admin)
}

/// Set up a quorum raffle in Drawing state with tickets sold and randomness requested.
pub fn setup_quorum_drawing_raffle(
    env: &Env,
    k: u32,
    oracles: &[Address],
) -> (ContractClient<'_>, Address, Address, u64) {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);

    let token_admin = Address::generate(env);
    let (token_addr, token_mint) = create_token(env, &token_admin);
    token_mint.mint(&creator, &1_000_000);

    let mut oracle_vec = Vec::new(env);
    for oracle in oracles {
        oracle_vec.push_back(oracle.clone());
    }

    let config = RaffleConfig {
        description: String::from_str(env, "quorum raffle"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr,
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Quorum(QuorumConfig {
            k,
            oracles: oracle_vec,
        }),
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[55u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();
    client.buy_tickets(&creator, &3);
    client.finalize_raffle();

    let request_id: u64 = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::RandomnessRequestId)
            .unwrap_or(0)
    });

    (client, contract_id, creator, request_id)
}

mod admin;
mod claim;
mod draw;
mod init;
mod invariants;
mod tickets;
