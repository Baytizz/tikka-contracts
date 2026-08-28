//! End-to-end protocol fee accounting invariants (#821).
//!
//! Tracks every balance across purchase → finalize → claim → withdraw_fees and
//! asserts the treasury receives exactly the documented fee rate — no double
//! counting via AccumulatedFees, no rounding surprises, and no over-withdrawal.

use crate::{
    DataKey, Error, RaffleConfig, RaffleInstance, RaffleInstanceClient, RaffleStatus,
    MAX_PROTOCOL_FEE_BP, MIN_TICKET_PRICE,
};
use raffle_shared::RandomnessSource;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, BytesN, Env, String, Vec,
};

/// Worked example from docs/FEE_MODEL.md (mirrored exactly in the test below).
const EXAMPLE_TICKET_PRICE: i128 = 100_000_000; // 100 XLM (7 decimals)
const EXAMPLE_PRIZE_AMOUNT: i128 = 800_000_000; // 800 XLM
const EXAMPLE_TICKET_COUNT: u32 = 10;
const EXAMPLE_FEE_BP: u32 = 250; // 2.5%

/// Floor division — matches ticket purchase fee collection in tickets.rs.
fn ticket_fee_per_unit(amount: i128, fee_bp: u32) -> i128 {
    amount * fee_bp as i128 / 10_000
}

/// Ceiling division — matches prize claim fee collection in claim.rs.
fn claim_fee(amount: i128, fee_bp: u32) -> i128 {
    (amount * fee_bp as i128 + 9_999) / 10_000
}

fn tier_gross(prize_amount: i128, tier_bp: u32, is_last_tier: bool, prior_tiers: &[u32]) -> i128 {
    if is_last_tier {
        let mut allocated = 0i128;
        for bp in prior_tiers {
            allocated += prize_amount * *bp as i128 / 10_000;
        }
        prize_amount - allocated
    } else {
        prize_amount * tier_bp as i128 / 10_000
    }
}

struct FeeExpectations {
    ticket_fees: i128,
    claim_fees: i128,
}

fn expected_fees(
    ticket_price: i128,
    ticket_count: u32,
    prize_amount: i128,
    tier_bps: &[u32],
    fee_bp: u32,
) -> FeeExpectations {
    let per_ticket = ticket_fee_per_unit(ticket_price, fee_bp);
    let ticket_fees = per_ticket * ticket_count as i128;

    let mut claim_fees = 0i128;
    let last = tier_bps.len().saturating_sub(1);
    for (idx, bp) in tier_bps.iter().enumerate() {
        let prior: Vec<u32> = tier_bps[..idx].to_vec();
        let gross = tier_gross(prize_amount, *bp, idx == last, &prior);
        claim_fees += claim_fee(gross, fee_bp);
    }

    FeeExpectations {
        ticket_fees,
        claim_fees,
    }
}

fn setup_raffle(
    env: &Env,
    fee_bp: u32,
    ticket_price: i128,
    max_tickets: u32,
    prize_amount: i128,
    tier_bps: &[u32],
) -> (
    RaffleInstanceClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
    TokenClient<'_>,
) {
    let factory = Address::generate(env);
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let treasury = Address::generate(env);
    let token_admin = Address::generate(env);

    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_sac = StellarAssetClient::new(env, &payment_token);
    let token = TokenClient::new(env, &payment_token);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);

    let mut prizes = Vec::new(env);
    for bp in tier_bps {
        prizes.push_back(*bp);
    }

    let config = RaffleConfig {
        description: String::from_str(env, "Fee accounting invariant"),
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx: max_tickets,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price,
        payment_token: payment_token.clone(),
        prize_amount,
        prizes,
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: fee_bp,
        treasury_address: if fee_bp > 0 {
            Some(treasury.clone())
        } else {
            None
        },
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[0xFE; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);

    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    token_sac.mint(&creator, &(prize_amount * 2));

    (client, payment_token, creator, treasury, contract_id, admin, token)
}

fn run_fee_lifecycle(
    env: &Env,
    fee_bp: u32,
    ticket_price: i128,
    max_tickets: u32,
    prize_amount: i128,
    tier_bps: &[u32],
) {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, payment_token, creator, treasury, contract_id, admin, token) =
        setup_raffle(env, fee_bp, ticket_price, max_tickets, prize_amount, tier_bps);

    let expectations = expected_fees(ticket_price, max_tickets, prize_amount, tier_bps, fee_bp);
    let total_expected_fees = expectations.ticket_fees + expectations.claim_fees;

    let treasury_start = token.balance(&treasury);
    let contract_start = token.balance(&contract_id);

    // Deposit prize and sell out tickets (one buyer per ticket for simplicity).
    client.deposit_prize();
    for ticket_idx in 0..max_tickets {
        let buyer = Address::generate(env);
        StellarAssetClient::new(env, &payment_token).mint(&buyer, &ticket_price);
        client.buy_tickets(&buyer, &1);
        let _ = ticket_idx;
    }

    client.finalize_raffle();

    env.ledger().set_timestamp(2_000);

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Finalized);

    let mut winners_paid = 0i128;
    for tier_idx in 0..raffle.winners.len() {
        let winner = raffle.winners.get(tier_idx).unwrap();
        let balance_before = token.balance(&winner);
        let gross = client.claim_prize(&winner, &tier_idx);
        let fee = claim_fee(gross, fee_bp);
        let net = gross - fee;
        winners_paid += net;
        assert_eq!(token.balance(&winner), balance_before + net);
    }

    // Withdraw accumulated fees to exhaustion.
    let mut withdrawn = 0i128;
    loop {
        let remaining = client.get_accumulated_fees();
        if remaining <= 0 {
            break;
        }
        let before = token.balance(&treasury);
        let result = client.try_withdraw_fees(&treasury, &remaining);
        if result.is_err() {
            break;
        }
        withdrawn += remaining;
        let after = token.balance(&treasury);
        assert_eq!(after - before, remaining);
    }

    let treasury_end = token.balance(&treasury);
    let contract_end = token.balance(&contract_id);
    let treasury_delta = treasury_end - treasury_start;

    if fee_bp == 0 {
        assert_eq!(total_expected_fees, 0);
        assert_eq!(treasury_delta, 0, "zero fee rate must move no fees");
        assert_eq!(client.get_accumulated_fees(), 0);
    } else {
        // Treasury must receive exactly the documented rate — not double via
        // direct transfer plus withdraw_fees replay.
        assert_eq!(
            treasury_delta, total_expected_fees,
            "treasury must receive exactly ticket + claim fees (fee_bp={fee_bp})"
        );

        // Winners received prize minus claim-time fees.
        let total_gross: i128 = tier_bps
            .iter()
            .enumerate()
            .map(|(idx, bp)| {
                let prior: Vec<u32> = tier_bps[..idx].to_vec();
                tier_gross(prize_amount, *bp, idx == tier_bps.len() - 1, &prior)
            })
            .sum();
        assert_eq!(winners_paid, total_gross - expectations.claim_fees);

        // Repeated withdraw cannot exceed fees actually collected.
        assert!(withdrawn <= total_expected_fees);
        let _ = admin;
        let second = client.try_withdraw_fees(&treasury, &1);
        assert_eq!(second, Err(Ok(Error::InsufficientAccumulatedFees)));

        // Contract residual: ticket revenue minus prize payouts minus any dust.
        let ticket_revenue = ticket_price * max_tickets as i128;
        let expected_contract = contract_start
            + ticket_revenue
            + prize_amount
            - winners_paid
            - withdrawn
            - expectations.ticket_fees
            - expectations.claim_fees;
        assert!(
            contract_end <= expected_contract.max(0),
            "contract residual must be zero or documented dust (fee_bp={fee_bp})"
        );
    }
}

#[test]
fn fee_model_worked_example_matches_lifecycle() {
    let env = Env::default();
    let per_ticket = ticket_fee_per_unit(EXAMPLE_TICKET_PRICE, EXAMPLE_FEE_BP);
    assert_eq!(per_ticket, 2_500_000); // 2.5 XLM
    assert_eq!(per_ticket * EXAMPLE_TICKET_COUNT as i128, 25_000_000);

    let claim = claim_fee(EXAMPLE_PRIZE_AMOUNT, EXAMPLE_FEE_BP);
    assert_eq!(claim, 20_000_000); // 20 XLM

    run_fee_lifecycle(
        &env,
        EXAMPLE_FEE_BP,
        EXAMPLE_TICKET_PRICE,
        EXAMPLE_TICKET_COUNT,
        EXAMPLE_PRIZE_AMOUNT,
        &[10_000],
    );
}

#[test]
fn fee_accounting_full_lifecycle_at_all_fee_settings() {
    let tier_bps = [6_000u32, 3_000, 1_000];
    let fee_settings = [0u32, 1, 100, MAX_PROTOCOL_FEE_BP];

    for fee_bp in fee_settings {
        let env = Env::default();
        run_fee_lifecycle(
            &env,
            fee_bp,
            MIN_TICKET_PRICE,
            3,
            MIN_TICKET_PRICE * 100,
            &tier_bps,
        );
    }
}
