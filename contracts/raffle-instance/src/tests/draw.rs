//! Unique-winner resolution tests (#826).
//!
//! Covers `resolve_unique_winner` in `helpers.rs`: when
//! `RaffleConfig.unique_winners` is set, the draw re-resolves any seed-selected
//! ticket whose owner has already won an earlier tier.  The strategy is
//! deterministic and replayable — the same seed, ticket layout, and draw order
//! always reproduce the same winner set (see `docs/RANDOMNESS.md`).
//!
//! Test scenarios:
//!
//! 1. N distinct owners with multiple tickets each, N tiers → each owner wins
//!    exactly once.
//! 2. A single owner holds every ticket → the draw terminates and the same
//!    address wins every tier (repeat fallback, see [`resolve_unique_winner`]).
//! 3. Two owners, three tiers → the draw terminates with exactly one repeat.
//! 4. `unique_winners = false` keeps the raw oracle selection unchanged.
//! 5. External/VRF: the same delivered seed replays identical winners.
//! 6. Recorded fairness indices resolve to the addresses that actually paid.
//! 7. Finalize stays within the committed instruction/memory budget at
//!    `MAX_PRIZES` tiers (mirrors `budget.rs::FINALIZE_MAX_PRIZES`).

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Budget, Ledger},
    token::StellarAssetClient,
    Address, BytesN, Env, String, Vec,
};

use crate::helpers::get_ticket_owner;
use crate::randomness::{build_vrf_proof_message, OracleSeedWinnerSelection};
use crate::{
    DataKey, RaffleConfig, RaffleInstance, RaffleInstanceClient, RaffleStatus, MAX_PRIZES,
    MAX_TICKETS_LIMIT, MIN_TICKET_PRICE,
};

/// Mock of the factory contract surface the instance invokes cross-contract:
/// global pause check, volume accounting, participant tracking, and the
/// leaderboard update emitted at finalize.
#[contract]
struct MockFactory;

#[contractimpl]
impl MockFactory {
    pub fn is_global_paused(_env: Env) -> bool {
        false
    }

    pub fn record_volume(_env: Env, _token: Address, _amount: i128) {}

    pub fn track_participant(_env: Env, _participant: Address) {}

    pub fn record_leaderboard_entry(
        _env: Env,
        _raffle_id: Address,
        _tickets: i128,
        _prize_amount: i128,
        _volume: i128,
    ) {
    }
}

/// Commodity baseline for the budget regression (`budget.rs::FINALIZE_MAX_PRIZES`).
const FINALIZE_MAX_PRIZES_BASELINE: (u64, u64) = (80_000_000, 24 * 1024 * 1024);
const TOLERANCE_FRACTION: f64 = 0.10;

fn build_prizes(env: &Env, tiers: u32) -> Vec<u32> {
    let each = 10_000u32 / tiers;
    let mut prizes = Vec::new(env);
    let mut sum = 0u32;
    for i in 0..tiers {
        if i + 1 == tiers {
            prizes.push_back(10_000 - sum);
        } else {
            prizes.push_back(each);
            sum += each;
        }
    }
    prizes
}

/// Spin up a raffle with `buyers` each purchasing `tickets_per_buyer` tickets,
/// using a deadline so the draw is triggered by `finalize_raffle` rather than a
/// sell-out (a sell-out auto-transitions to `Drawing` and sets `DrawingLock`).
///
/// Returns the client and contract id. The raffle is `Active` (prize deposited,
/// tickets sold) but not yet finalized.
fn setup_raffle(
    env: &Env,
    buyers: &[Address],
    tickets_per_buyer: u32,
    max_tickets: u32,
    tiers: u32,
    unique_winners: bool,
    external: bool,
) -> (RaffleInstanceClient<'_>, Address) {
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);

    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let oracle = Address::generate(env);

    let token_admin = Address::generate(env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token = StellarAssetClient::new(env, &payment_token);
    token.mint(&creator, &1_000_000_000_000);
    for buyer in buyers {
        token.mint(buyer, &1_000_000_000_000);
    }

    let config = RaffleConfig {
        description: String::from_str(env, "unique winners"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets,
        max_tickets_per_tx: 1_000,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * max_tickets as i128,
        prizes: build_prizes(env, tiers),
        randomness_source: if external {
            raffle_shared::RandomnessSource::External
        } else {
            raffle_shared::RandomnessSource::Internal
        },
        oracle_address: if external { Some(oracle) } else { None },
        protocol_fee_bp: 0,
        treasury_address: Some(Address::generate(env)),
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[9u8; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(300),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners,
        bundles: Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    for buyer in buyers {
        client.buy_tickets(buyer, &tickets_per_buyer).unwrap();
    }
    (client, contract_id)
}

/// Internal mode: advance past the deadline and finalize in one call.
fn finalize_internal(env: &Env, client: &RaffleInstanceClient<'_>) {
    env.ledger().set_timestamp(5_000);
    client.finalize_raffle().unwrap();
}

/// External/VRF mode: finalize to request randomness, then deliver a fixed seed
/// signed by a fresh key over the per-raffle proof message.
fn finalize_external_with_seed(
    env: &Env,
    client: &RaffleInstanceClient<'_>,
    contract_id: &Address,
    seed: u64,
) {
    env.ledger().set_timestamp(5_000);
    client.finalize_raffle().unwrap();

    let request_id: u64 = env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::RandomnessRequestId)
            .unwrap()
    });

    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let verifying = signing_key.verifying_key();
    let message = env.as_contract(contract_id, || {
        build_vrf_proof_message(env, request_id, seed)
    });
    let signature = signing_key.sign(message.as_slice());

    client
        .provide_randomness(
            &seed,
            &BytesN::from_array(env, &verifying.to_bytes()),
            &BytesN::from_array(env, &signature.to_bytes()),
            &request_id,
        )
        .unwrap();
}

/// Derive the final winner addresses from the recorded fairness data plus
/// on-chain ticket storage. This is exactly the replay path an off-chain
/// auditor would use (`winning_ticket_indices[i]` is a zero-based offset into
/// `ticket_ids`, so the winning ticket is `index + 1`).
fn recorded_winners(
    env: &Env,
    client: &RaffleInstanceClient<'_>,
    contract_id: &Address,
) -> std::vec::Vec<Address> {
    let fairness = client.get_fairness_data().unwrap();
    let mut winners = std::vec::Vec::new();
    for i in 0..fairness.winning_ticket_indices.len() {
        let index = fairness.winning_ticket_indices.get(i).unwrap();
        let owner = env.as_contract(contract_id, || get_ticket_owner(env, index + 1).unwrap());
        winners.push(owner);
    }
    winners
}

fn assert_finalized(client: &RaffleInstanceClient<'_>) {
    assert_eq!(client.get_raffle().unwrap().status, RaffleStatus::Finalized);
}

#[test]
fn five_distinct_owners_ten_tickets_five_tiers_all_distinct_winners() {
    let env = Env::default();
    env.mock_all_auths();

    let owners: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    let (client, _contract_id) = setup_raffle(&env, &owners, 2, 10, 5, true, false);
    finalize_internal(&env, &client);
    assert_finalized(&client);

    let winners = client.get_fairness_data().unwrap();
    let addresses = recorded_winners(&env, &client, &_contract_id);
    assert_eq!(addresses.len(), 5);
    for owner in &owners {
        let count = addresses.iter().filter(|w| w == owner).count();
        assert_eq!(
            count, 1,
            "each of the five owners must win exactly one tier"
        );
    }
    assert!(winners.unique_winners);
}

#[test]
fn single_owner_every_ticket_terminates_with_repeat() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let owners = std::vec![owner.clone()];
    let (client, _contract_id) = setup_raffle(&env, &owners, 10, 20, 3, true, false);
    finalize_internal(&env, &client);
    assert_finalized(&client);

    let fairness = client.get_fairness_data().unwrap();
    assert!(fairness.unique_winners);
    assert_eq!(fairness.winning_ticket_indices.len(), 3, "three tiers");

    let addresses = recorded_winners(&env, &client, &_contract_id);
    assert_eq!(addresses.len(), 3);
    assert!(
        addresses.iter().all(|w| w == &owner),
        "a single-owner raffle must repeat the owner on every tier rather than panic"
    );
}

#[test]
fn two_owners_three_tiers_terminates_with_exactly_one_repeat() {
    let env = Env::default();
    env.mock_all_auths();

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let owners = std::vec![a.clone(), b.clone()];
    let (client, _contract_id) = setup_raffle(&env, &owners, 3, 10, 3, true, false);
    finalize_internal(&env, &client);
    assert_finalized(&client);

    let addresses = recorded_winners(&env, &client, &_contract_id);
    assert_eq!(addresses.len(), 3);
    assert!(
        addresses.iter().all(|w| w == &a || w == &b),
        "winners must be drawn from the two paying owners"
    );
    let distinct: std::collections::HashSet<Address> = addresses.iter().cloned().collect();
    assert_eq!(
        distinct.len(),
        2,
        "two owners across three tiers must yield exactly one repeat, not a hang"
    );
}

#[test]
fn unique_winners_disabled_records_raw_selection() {
    let env = Env::default();
    env.mock_all_auths();

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let owners = std::vec![a.clone(), b.clone()];
    let (client, _contract_id) = setup_raffle(&env, &owners, 3, 10, 3, false, false);
    finalize_internal(&env, &client);
    assert_finalized(&client);

    let fairness = client.get_fairness_data().unwrap();
    assert!(!fairness.unique_winners);

    let raffle = client.get_raffle().unwrap();
    let selector = OracleSeedWinnerSelection::new(fairness.seed);
    let expected = selector.select_winner_indices_pure(raffle.tickets_sold, 3);

    let recorded: std::vec::Vec<u32> = (0..fairness.winning_ticket_indices.len())
        .map(|i| fairness.winning_ticket_indices.get(i).unwrap())
        .collect();
    assert_eq!(
        recorded, expected,
        "unique_winners=false must keep the raw rejection-sampled selection unchanged"
    );
}

#[test]
fn same_seed_replays_identical_winners() {
    let env = Env::default();
    env.mock_all_auths();

    let owners: std::vec::Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();

    let (client_a, contract_a) = setup_raffle(&env, &owners, 2, 10, 4, true, true);
    finalize_external_with_seed(&env, &client_a, &contract_a, 424_242);
    assert_finalized(&client_a);

    let (client_b, contract_b) = setup_raffle(&env, &owners, 2, 10, 4, true, true);
    finalize_external_with_seed(&env, &client_b, &contract_b, 424_242);
    assert_finalized(&client_b);

    let winners_a = recorded_winners(&env, &client_a, &contract_a);
    let winners_b = recorded_winners(&env, &client_b, &contract_b);
    assert_eq!(
        winners_a, winners_b,
        "identical seed must replay identical winners"
    );

    let fa = client_a.get_fairness_data().unwrap();
    let fb = client_b.get_fairness_data().unwrap();
    assert_eq!(fa.seed, 424_242);
    assert_eq!(fb.seed, 424_242);
    assert!(fa.unique_winners);
    assert!(fb.unique_winners);

    let idx_a: std::vec::Vec<u32> = (0..fa.winning_ticket_indices.len())
        .map(|i| fa.winning_ticket_indices.get(i).unwrap())
        .collect();
    let idx_b: std::vec::Vec<u32> = (0..fb.winning_ticket_indices.len())
        .map(|i| fb.winning_ticket_indices.get(i).unwrap())
        .collect();
    assert_eq!(idx_a, idx_b, "winning indices must be reproducible");
}

#[test]
fn fairness_indices_resolve_to_who_was_paid() {
    let env = Env::default();
    env.mock_all_auths();

    let buyers: std::vec::Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    let (client, contract_id) = setup_raffle(&env, &buyers, 3, 15, 3, true, false);
    finalize_internal(&env, &client);
    assert_finalized(&client);

    let fairness = client.get_fairness_data().unwrap();
    let winners = recorded_winners(&env, &client, &contract_id);
    assert_eq!(winners.len(), 3);

    for i in 0..fairness.winning_ticket_indices.len() {
        let index = fairness.winning_ticket_indices.get(i).unwrap();
        let owner = env.as_contract(&contract_id, || get_ticket_owner(&env, index + 1).unwrap());
        assert!(
            buyers.contains(&owner),
            "winning ticket {index} must belong to an address that actually paid"
        );
        assert_eq!(
            owner, winners[i as usize],
            "recorded index must match the resolved winner"
        );
    }

    let distinct: std::collections::HashSet<Address> = winners.iter().cloned().collect();
    assert_eq!(
        distinct.len(),
        3,
        "three owners across three tiers: every owner wins once"
    );
}

#[test]
fn unique_winners_max_prizes_within_budget_baseline() {
    const TICKETS_PER_OWNER: u32 = 999;
    const TIERS: u32 = MAX_PRIZES;

    let env = Env::default();
    env.mock_all_auths();

    let owners: std::vec::Vec<Address> = (0..100).map(|_| Address::generate(&env)).collect();
    let total_bought = owners.len() as u32 * TICKETS_PER_OWNER; // 99_900 < MAX_TICKETS_LIMIT
    assert!(total_bought < MAX_TICKETS_LIMIT);

    let (client, _contract_id) = setup_raffle(
        &env,
        &owners,
        TICKETS_PER_OWNER,
        MAX_TICKETS_LIMIT,
        TIERS,
        true,
        false,
    );

    env.ledger().set_timestamp(5_000);
    env.cost_estimate().budget().reset_default();
    client.finalize_raffle().unwrap();
    let budget = env.cost_estimate().budget();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();

    let (cpu_baseline, mem_baseline) = FINALIZE_MAX_PRIZES_BASELINE;
    let cpu_limit = ((cpu_baseline as f64) * (1.0 + TOLERANCE_FRACTION)).ceil() as u64;
    let mem_limit = ((mem_baseline as f64) * (1.0 + TOLERANCE_FRACTION)).ceil() as u64;
    assert!(
        cpu <= cpu_limit,
        "unique-winner finalize cpu {cpu} exceeded baseline {cpu_baseline} + {:.0}% (limit {cpu_limit})",
        TOLERANCE_FRACTION * 100.0
    );
    assert!(
        mem <= mem_limit,
        "unique-winner finalize memory {mem} exceeded baseline {mem_baseline} + {:.0}% (limit {mem_limit})",
        TOLERANCE_FRACTION * 100.0
    );

    assert_finalized(&client);
    let fairness = client.get_fairness_data().unwrap();
    assert_eq!(fairness.winning_ticket_indices.len(), TIERS as usize);
}
