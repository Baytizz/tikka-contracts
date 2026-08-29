//! Refund-path lifecycle tests (#827).
//!
//! Every cancellation and failure path is driven end-to-end and must settle to
//! the same state: every ticket holder recovers exactly what they paid, the
//! creator recovers exactly the deposited prize (in the prize token), double
//! refunds are rejected, refund ordering never changes the outcome, and the
//! contract ends with a zero payment-token balance.
//!
//! The refund-solvency invariant (refunds never exceed what the contract
//! holds) is asserted after every refund operation using the helper in
//! [`super::invariants::assert_refund_solvency`].

use raffle_shared::{
    constants::{MIN_TICKET_PRICE, ORACLE_TIMEOUT_LEDGERS},
    CancelReason, RaffleConfig, RaffleStatus, RandomnessSource,
};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, testutils::Ledger, token::StellarAssetClient,
    Address, BytesN, Env, String, Vec,
};

use super::invariants::assert_refund_solvency;
use crate::{Error, RaffleInstance, RaffleInstanceClient};

/// Minimal live factory stub. The raffle contract calls back into the factory
/// during buys and finalization (`is_global_paused`, `record_volume`,
/// `track_participant`, `record_leaderboard_entry`), so it must be a real
/// registered contract in these tests.
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

struct RaffleOpts {
    max_tickets: u32,
    max_tickets_per_tx: u32,
    min_tickets: u32,
    no_deadline: bool,
    end_time: u64,
    external: bool,
    early_bird_percentage: u32,
    early_bird_discount_bp: u32,
}

impl RaffleOpts {
    fn open(max_tickets: u32) -> Self {
        Self {
            max_tickets,
            max_tickets_per_tx: max_tickets,
            min_tickets: 1,
            no_deadline: true,
            end_time: 0,
            external: false,
            early_bird_percentage: 0,
            early_bird_discount_bp: 0,
        }
    }

    fn deadlined(max_tickets: u32, min_tickets: u32, end_time: u64) -> Self {
        Self {
            max_tickets,
            max_tickets_per_tx: max_tickets,
            min_tickets,
            no_deadline: false,
            end_time,
            external: false,
            early_bird_percentage: 0,
            early_bird_discount_bp: 0,
        }
    }
}

/// Handles for a live raffle instance plus its payment/prize token.
struct Lifecycle<'a> {
    client: RaffleInstanceClient<'a>,
    contract_id: Address,
    token: StellarAssetClient<'a>,
    payment_token: Address,
    creator: Address,
    admin: Address,
    oracle: Address,
    prize_amount: i128,
}

fn setup(env: &Env, opts: &RaffleOpts) -> Lifecycle<'_> {
    let token_admin = Address::generate(env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token = StellarAssetClient::new(env, &payment_token);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);

    let factory_id = env.register(MockFactory, ());
    let creator = Address::generate(env);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    let config = RaffleConfig {
        description: String::from_str(env, "refund-path test raffle"),
        end_time: opts.end_time,
        no_deadline: opts.no_deadline,
        max_tickets: opts.max_tickets,
        max_tickets_per_tx: opts.max_tickets_per_tx,
        max_tickets_per_address: 0,
        min_tickets: opts.min_tickets,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token,
        prize_amount: opts.max_tickets as i128 * MIN_TICKET_PRICE,
        prizes: Vec::from_array(env, [10_000]),
        randomness_source: if opts.external {
            RandomnessSource::External
        } else {
            RandomnessSource::Internal
        },
        oracle_address: if opts.external {
            Some(oracle.clone())
        } else {
            None
        },
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[5; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: opts.early_bird_percentage,
        early_bird_discount_bp: opts.early_bird_discount_bp,
        category: None,
        unique_winners: false,
        bundles: Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory_id, &admin, &creator, &config).unwrap();
    client.deposit_prize().unwrap();

    let prize_amount = client.get_raffle().unwrap().prize_amount;
    Lifecycle {
        client,
        contract_id,
        token,
        payment_token,
        creator,
        admin,
        oracle,
        prize_amount,
    }
}

/// Buy `quantity` tickets for `buyer` and return the exact amount the buyer
/// paid, measured as the token balance delta around the purchase. This keeps
/// the assertions robust whether or not the early-bird discount pricing is
/// actually applied at buy time.
fn buy_and_paid(env: &Env, lc: &Lifecycle<'_>, buyer: &Address, quantity: u32) -> i128 {
    let before = lc.token.balance(buyer);
    lc.client.buy_tickets(buyer, &quantity).unwrap();
    let after = lc.token.balance(buyer);
    before - after
}

/// Immutable snapshot of a fully settled raffle: every party is made whole and
/// the contract holds nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SettledSnapshot {
    status: RaffleStatus,
    contract_balance: i128,
}

/// Every cancellation and failure path is settled by refunding every ticket id
/// and then the prize, asserting the solvency invariant around each refund.
fn settle_everything(env: &Env, lc: &Lifecycle<'_>, ticket_ids: &[u32]) -> SettledSnapshot {
    let mut owing: std::vec::Vec<u32> = ticket_ids.to_vec();
    let per_ticket = lc.client.get_raffle().unwrap().ticket_price;

    for id in ticket_ids.iter().copied() {
        assert_eq!(owing[0], id, "tickets must be refunded in id order");
        owing.remove(0);
        assert_refund_solvency(
            env,
            &lc.contract_id,
            &lc.payment_token,
            &lc.payment_token,
            &owing,
            per_ticket,
            if lc.client.get_raffle().unwrap().prize_deposited {
                lc.prize_amount
            } else {
                0
            },
        );
        let recovered = lc.client.refund_ticket(&id).unwrap();
        assert!(recovered >= per_ticket);
        assert_refund_solvency(
            env,
            &lc.contract_id,
            &lc.payment_token,
            &lc.payment_token,
            &owing,
            per_ticket,
            if lc.client.get_raffle().unwrap().prize_deposited {
                lc.prize_amount
            } else {
                0
            },
        );
    }

    lc.client.refund_prize().unwrap();
    assert_refund_solvency(
        env,
        &lc.contract_id,
        &lc.payment_token,
        &lc.payment_token,
        &[],
        per_ticket,
        0,
    );

    let raffle = lc.client.get_raffle().unwrap();
    SettledSnapshot {
        status: raffle.status,
        contract_balance: lc.token.balance(&lc.contract_id),
    }
}

/// The distinct ways a raffle can reach a Cancelled or Failed terminal state.
#[derive(Clone, Copy, Debug)]
enum TerminalPath {
    CreatorCancelled,
    AdminCancelled,
    MinTicketsCancelled,
    OracleTimeout,
    ZeroTicketsSold,
    MinTicketsNotMet,
}

/// Build the scenario for a terminal path, drive it there, settle everything,
/// verify every party is made whole, and return the final snapshot.
fn settle_path(path: TerminalPath) -> SettledSnapshot {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (opts, buys) = match path {
        TerminalPath::CreatorCancelled => (RaffleOpts::open(6), vec![(0, 1), (1, 1), (1, 2)]),
        TerminalPath::AdminCancelled => (RaffleOpts::open(6), vec![(0, 3)]),
        TerminalPath::MinTicketsCancelled => (RaffleOpts::open(6), vec![(0, 1)]),
        TerminalPath::OracleTimeout => (RaffleOpts::open(4), vec![(0, 4)]),
        TerminalPath::ZeroTicketsSold => (RaffleOpts::deadlined(6, 2, 2_000), std::vec::Vec::new()),
        TerminalPath::MinTicketsNotMet => {
            let mut opts = RaffleOpts::deadlined(6, 5, 2_000);
            opts.max_tickets_per_tx = 2;
            (opts, vec![(0, 2)])
        }
    };

    let lc = setup(&env, &opts);
    let buyers: std::vec::Vec<Address> = (0..2).map(|_| Address::generate(&env)).collect();
    for b in &buyers {
        lc.token.mint(b, &1_000_000);
    }

    let creator_initial = lc.token.balance(&lc.creator);
    let buyer_initials: std::vec::Vec<i128> = buyers.iter().map(|b| lc.token.balance(b)).collect();

    let mut ticket_ids: std::vec::Vec<u32> = std::vec::Vec::new();
    for (buyer_idx, quantity) in buys {
        let refunding_buyer = &buyers[buyer_idx];
        let next_id = ticket_ids.len() as u32 + 1;
        let _ = buy_and_paid(&env, &lc, refunding_buyer, quantity);
        for offset in 0..quantity {
            ticket_ids.push(next_id + offset);
        }
    }

    match path {
        TerminalPath::CreatorCancelled => {
            lc.client
                .cancel_raffle(&CancelReason::CreatorCancelled)
                .unwrap();
        }
        TerminalPath::AdminCancelled => {
            lc.client
                .cancel_raffle(&CancelReason::AdminCancelled)
                .unwrap();
        }
        TerminalPath::MinTicketsCancelled => {
            lc.client
                .cancel_raffle(&CancelReason::MinTicketsNotMet)
                .unwrap();
        }
        TerminalPath::OracleTimeout => {
            // Selling out the four tickets triggered
            // `trigger_randomness_fallback(do_refund=true)` eligibility; the
            // timeout window is measured in ledgers, so advance the sequence.
            let requested_at = env.ledger().sequence();
            env.ledger()
                .set_sequence_number(requested_at + ORACLE_TIMEOUT_LEDGERS + 1);
            lc.client
                .trigger_randomness_fallback(&lc.creator, &true)
                .unwrap();
        }
        TerminalPath::ZeroTicketsSold | TerminalPath::MinTicketsNotMet => {
            env.ledger().set_timestamp(5_000);
            lc.client.finalize_raffle().unwrap();
        }
    }

    match path {
        TerminalPath::OracleTimeout => {
            assert_eq!(
                lc.client.get_raffle().unwrap().status,
                RaffleStatus::Cancelled
            );
        }
        TerminalPath::ZeroTicketsSold | TerminalPath::MinTicketsNotMet => {
            assert_eq!(lc.client.get_raffle().unwrap().status, RaffleStatus::Failed);
        }
        _ => {
            assert_eq!(
                lc.client.get_raffle().unwrap().status,
                RaffleStatus::Cancelled
            );
        }
    }

    let snapshot = settle_everything(&env, &lc, &ticket_ids);

    assert_eq!(
        lc.token.balance(&lc.contract_id),
        0,
        "contract fully settled"
    );
    assert_eq!(
        lc.token.balance(&lc.creator),
        creator_initial,
        "creator recovers the deposited prize exactly"
    );
    for (buyer_idx, b) in buyers.iter().enumerate() {
        assert_eq!(
            lc.token.balance(b),
            buyer_initials[buyer_idx],
            "buyer {buyer_idx} recovers exactly what they paid"
        );
    }

    snapshot
}

#[test]
fn every_cancel_and_failure_path_settles_identically() {
    let expected = SettledSnapshot {
        status: RaffleStatus::Cancelled,
        contract_balance: 0,
    };
    for (i, path) in [
        TerminalPath::CreatorCancelled,
        TerminalPath::AdminCancelled,
        TerminalPath::MinTicketsCancelled,
        TerminalPath::OracleTimeout,
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            settle_path(*path),
            expected,
            "cancel path #{i} must settle to the same fully-settled snapshot"
        );
    }

    let expected_failed = SettledSnapshot {
        status: RaffleStatus::Failed,
        contract_balance: 0,
    };
    for (i, path) in [
        TerminalPath::ZeroTicketsSold,
        TerminalPath::MinTicketsNotMet,
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            settle_path(*path),
            expected_failed,
            "failure path #{i} must settle to the same fully-settled snapshot"
        );
    }
}

#[test]
fn each_holder_recovers_exactly_what_they_paid() {
    let env = Env::default();
    env.mock_all_auths();
    let lc = setup(&env, &RaffleOpts::open(12));

    let holders = (0..3).map(|_| Address::generate(&env)).collect::<Vec<_>>();
    for h in &holders {
        lc.token.mint(h, &1_000_000);
    }

    let paid: std::vec::Vec<i128> = holders
        .iter()
        .zip([1u32, 2, 3])
        .map(|(h, qty)| buy_and_paid(&env, &lc, h, qty))
        .collect();

    lc.client
        .cancel_raffle(&CancelReason::CreatorCancelled)
        .unwrap();

    let mut ticket_ids = 1u32..=6;
    let mut total_recovered = 0i128;
    for (idx, (holder, bought)) in holders.iter().zip(paid.iter()).enumerate() {
        let mut recovered = 0i128;
        for _ in 0..[1, 2, 3][idx] {
            let id = ticket_ids.next().unwrap();
            recovered += lc.client.refund_ticket(&id).unwrap();
        }
        assert_eq!(
            recovered, *bought,
            "holder {idx} got back exactly their payment"
        );
        total_recovered += recovered;
    }

    lc.client.refund_prize().unwrap();
    assert_eq!(lc.token.balance(&lc.contract_id), 0);
    assert_eq!(total_recovered, paid.iter().sum());
}

#[test]
fn creator_recovers_deposited_prize_in_the_prize_token() {
    let env = Env::default();
    env.mock_all_auths();
    let lc = setup(&env, &RaffleOpts::open(3));

    let buyer = Address::generate(&env);
    lc.token.mint(&buyer, &1_000_000);
    buy_and_paid(&env, &lc, &buyer, 1);

    let raffle = lc.client.get_raffle().unwrap();
    assert_eq!(
        raffle.payment_token, raffle.prize_token,
        "prize token is wired to the payment token today"
    );
    let prize_token = raffle.prize_token;
    let prize_tc = StellarAssetClient::new(&env, &prize_token);

    let creator_before = prize_tc.balance(&lc.creator);
    lc.client
        .cancel_raffle(&CancelReason::CreatorCancelled)
        .unwrap();

    lc.client.refund_prize().unwrap();

    let raffle = lc.client.get_raffle().unwrap();
    assert!(!raffle.prize_deposited, "prize obligation cleared");
    assert_eq!(
        prize_tc.balance(&lc.creator) - creator_before,
        lc.prize_amount,
        "creator receives the full prize in the prize token"
    );
}

#[test]
fn double_refund_of_the_same_ticket_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let lc = setup(&env, &RaffleOpts::open(3));

    let buyer = Address::generate(&env);
    lc.token.mint(&buyer, &1_000_000);
    buy_and_paid(&env, &lc, &buyer, 1);

    lc.client
        .cancel_raffle(&CancelReason::CreatorCancelled)
        .unwrap();

    let first = lc.client.refund_ticket(&1).unwrap();
    assert_eq!(first, MIN_TICKET_PRICE);
    assert_eq!(
        lc.client.try_refund_ticket(&1).err(),
        Some(Ok(Error::PrizeAlreadyClaimed))
    );
    lc.client.refund_prize().unwrap();
}

#[test]
fn refunds_are_rejected_while_active_and_while_drawing() {
    let env = Env::default();
    env.mock_all_auths();
    let lc = setup(&env, &RaffleOpts::open(3));

    let buyer = Address::generate(&env);
    lc.token.mint(&buyer, &1_000_000);
    buy_and_paid(&env, &lc, &buyer, 1);

    assert_eq!(
        lc.client.try_refund_ticket(&1).err(),
        Some(Ok(Error::InvalidStatus))
    );
    assert_eq!(
        lc.client.try_refund_prize().err(),
        Some(Ok(Error::InvalidStatus))
    );

    // A second raffle that sells out enters Drawing while randomness is pending.
    let env = Env::default();
    env.mock_all_auths();
    let opts = RaffleOpts {
        external: true,
        ..RaffleOpts::open(1)
    };
    let lc = setup(&env, &opts);
    let buyer = Address::generate(&env);
    lc.token.mint(&buyer, &1_000_000);
    lc.client.buy_tickets(&buyer, &1).unwrap();
    assert_eq!(
        lc.client.get_raffle().unwrap().status,
        RaffleStatus::Drawing
    );
    assert_eq!(
        lc.client.try_refund_ticket(&1).err(),
        Some(Ok(Error::InvalidStatus))
    );
    assert_eq!(
        lc.client.try_refund_prize().err(),
        Some(Ok(Error::InvalidStatus))
    );
}

#[test]
fn refund_of_a_nonexistent_ticket_returns_ticket_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let lc = setup(&env, &RaffleOpts::open(3));

    lc.client
        .cancel_raffle(&CancelReason::CreatorCancelled)
        .unwrap();
    assert_eq!(
        lc.client.try_refund_ticket(&99).err(),
        Some(Ok(Error::TicketNotFound))
    );
}

#[test]
fn refund_ordering_does_not_change_the_outcome() {
    let settle = |env: &Env, prize_first: bool| -> (i128, i128, i128) {
        env.mock_all_auths();
        let lc = setup(env, &RaffleOpts::open(4));
        let alice = Address::generate(env);
        let bob = Address::generate(env);
        lc.token.mint(&alice, &1_000_000);
        lc.token.mint(&bob, &1_000_000);
        buy_and_paid(env, &lc, &alice, 2);
        buy_and_paid(env, &lc, &bob, 1);
        lc.client
            .cancel_raffle(&CancelReason::CreatorCancelled)
            .unwrap();

        if prize_first {
            lc.client.refund_prize().unwrap();
        }
        for id in 1..=3 {
            lc.client.refund_ticket(&id).unwrap();
        }
        if !prize_first {
            lc.client.refund_prize().unwrap();
        }

        (
            lc.token.balance(&lc.contract_id),
            lc.token.balance(&alice),
            lc.token.balance(&bob),
        )
    };

    let env_a = Env::default();
    let (bal_a, alice_a, bob_a) = settle(&env_a, false);
    let env_b = Env::default();
    let (bal_b, alice_b, bob_b) = settle(&env_b, true);

    assert_eq!((bal_a, alice_a, bob_a), (bal_b, alice_b, bob_b));
    assert_eq!(bal_a, 0, "contract is empty either way");
}

#[test]
fn early_bird_and_full_price_buyers_both_settle_exactly() {
    let env = Env::default();
    env.mock_all_auths();
    let opts = RaffleOpts {
        early_bird_ticket_percentage: 50,
        early_bird_discount_bp: 2_000,
        ..RaffleOpts::open(12)
    };
    let lc = setup(&env, &opts);

    let early = Address::generate(&env);
    let full = Address::generate(&env);
    lc.token.mint(&early, &1_000_000);
    lc.token.mint(&full, &1_000_000);

    let early_paid = buy_and_paid(&env, &lc, &early, 3);
    let full_paid = buy_and_paid(&env, &lc, &full, 2);

    lc.client
        .cancel_raffle(&CancelReason::CreatorCancelled)
        .unwrap();

    let mut ticket_ids = 1u32..=5;
    let mut early_recovered = 0i128;
    let mut full_recovered = 0i128;
    for _ in 0..3 {
        early_recovered += lc
            .client
            .refund_ticket(&ticket_ids.next().unwrap())
            .unwrap();
    }
    for _ in 0..2 {
        full_recovered += lc
            .client
            .refund_ticket(&ticket_ids.next().unwrap())
            .unwrap();
    }
    lc.client.refund_prize().unwrap();

    assert_eq!(
        early_recovered, early_paid,
        "early-bird buyer recovers exactly what they paid"
    );
    assert_eq!(
        full_recovered, full_paid,
        "full-price buyer recovers exactly"
    );
    assert_eq!(lc.token.balance(&lc.contract_id), 0);
}

#[test]
fn solvency_is_asserted_after_every_partial_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let lc = setup(&env, &RaffleOpts::open(8));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    lc.token.mint(&alice, &1_000_000);
    lc.token.mint(&bob, &1_000_000);
    buy_and_paid(&env, &lc, &alice, 3);
    buy_and_paid(&env, &lc, &bob, 2);

    lc.client
        .cancel_raffle(&CancelReason::AdminCancelled)
        .unwrap();

    let per_ticket = lc.client.get_raffle().unwrap().ticket_price;
    let mut owing: std::vec::Vec<u32> = (1..=5).collect();

    for id in 1..=5 {
        owing.retain(|t| *t != id);
        assert_refund_solvency(
            &env,
            &lc.contract_id,
            &lc.payment_token,
            &lc.payment_token,
            &owing,
            per_ticket,
            lc.prize_amount,
        );
        lc.client.refund_ticket(&id).unwrap();
        assert_refund_solvency(
            &env,
            &lc.contract_id,
            &lc.payment_token,
            &lc.payment_token,
            &owing,
            per_ticket,
            lc.prize_amount,
        );
    }

    lc.client.refund_prize().unwrap();
    assert_refund_solvency(
        &env,
        &lc.contract_id,
        &lc.payment_token,
        &lc.payment_token,
        &[],
        per_ticket,
        0,
    );
}
