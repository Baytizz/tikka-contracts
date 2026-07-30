use soroban_sdk::{auth::InvokerContractAuthEntry, Address, Env, IntoVal, Symbol, Val, Vec};

use crate::events::{RaffleFinalized, WinnerDrawn};
use crate::randomness::{OracleSeedWinnerSelection, WinnerSelectionStrategy};
use crate::{
    get_ticket_owner, write_raffle, DataKey, Error, FairnessMetadata, Raffle, RaffleStatus,
    RandomnessType,
};

fn address_already_won(winners: &Vec<Address>, addr: &Address) -> bool {
    for i in 0..winners.len() {
        if winners.get(i).ok() == Some(addr.clone()) {
            return true;
        }
    }
    false
}

fn resolve_unique_winner(
    env: &Env,
    seed: u64,
    tier: u32,
    total_tickets: u32,
    winners_so_far: &Vec<Address>,
    initial_index: u32,
) -> u32 {
    let initial_owner = match get_ticket_owner(env, initial_index + 1) {
        Some(o) => o,
        None => return initial_index,
    };
    if !address_already_won(winners_so_far, &initial_owner) {
        return initial_index;
    }

    let mut attempt: u32 = 0;
    while attempt < total_tickets {
        let mut s = seed
            .wrapping_add((tier as u64) << 32)
            .wrapping_add(attempt as u64);
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let idx = (s % total_tickets as u64) as u32;
        if let Some(owner) = get_ticket_owner(env, idx + 1) {
            if !address_already_won(winners_so_far, &owner) {
                return idx;
            }
        }
        attempt += 1;
    }

    for ticket_id in 1..=total_tickets {
        if let Some(owner) = get_ticket_owner(env, ticket_id) {
            if !address_already_won(winners_so_far, &owner) {
                return ticket_id - 1;
            }
        }
    }

    initial_index
}

/// Finalize the raffle using a pre-computed `u64` seed.
///
/// This is the common finalization path shared by all three randomness modes
/// (`Internal`, `External`/VRF, and `Fallback`).  The caller selects the
/// appropriate seed and [`RandomnessType`] label before calling this function.
///
/// ## What this function does
///
/// 1. Validates `tickets_sold > 0` and `prizes.len() ≤ tickets_sold`.
/// 2. Uses [`OracleSeedWinnerSelection`] to pick `prizes.len()` distinct
///    winning ticket indices using rejection sampling (no modulo bias).
/// 3. Resolves each winning index to a ticket owner via
///    [`get_ticket_owner`] and emits a [`events::WinnerDrawn`] event per
///    winner.
/// 4. Writes [`FairnessMetadata`] to **persistent** storage under
///    [`DataKey::RandomnessSeed`] so it survives ledger-entry expiry and can
///    be queried by [`get_fairness_data`](crate::views::get_fairness_data).
/// 5. Sets `raffle.status = Finalized`, records `winners`,
///    `claimed_winners`, and `finalized_at`.
/// 6. Clears `RandomnessRequested`, `RandomnessRequestId`,
///    `RandomnessRequestLedger`, and sets `DrawingLock = false`.
/// 7. Emits [`events::RaffleFinalized`].
///
/// # Parameters
///
/// - `seed` — The 64-bit random seed to use for winner selection.
/// - `randomness_type` — Label for the audit trail (PRNG, VRF, or Fallback).
///
/// # Errors
///
/// - [`Error::NoTicketsSold`] — `tickets_sold == 0`.
/// - [`Error::MorePrizesThanTickets`] — more prize tiers than tickets sold.
/// - [`Error::NoActiveTickets`] — zero tickets found (should not happen in
///   practice if the above checks pass).
/// - [`Error::InvalidIndex`] — winner index out of range.
/// - [`Error::TicketNotFound`] — ticket record missing for a winning index.
/// - [`Error::ArithmeticOverflow`] — overflow in prize calculation.
///
/// # Events
///
/// - [`events::WinnerDrawn`] — emitted once per winner.
/// - [`events::RaffleFinalized`] — emitted after all winners are resolved.
///
/// See also: [`docs/EVENTS.md`](../../../../docs/EVENTS.md) — `WinnerDrawn`,
/// `RaffleFinalized`.
pub(crate) fn do_finalize_with_seed(
    env: &Env,
    mut raffle: Raffle,
    seed: u64,
    randomness_type: RandomnessType,
) -> Result<(), Error> {
    let total_tickets = raffle.tickets_sold;
    if total_tickets == 0 {
        return Err(Error::NoTicketsSold);
    }
    if raffle.prizes.len() > total_tickets {
        return Err(Error::MorePrizesThanTickets);
    }

    let selector = OracleSeedWinnerSelection::new(seed);
    let mut winning_ticket_ids =
        selector.select_winner_indices(env, total_tickets, raffle.prizes.len());
    let mut winners = Vec::new(env);

    for i in 0..winning_ticket_ids.len() {
        let mut idx = winning_ticket_ids.get(i).ok_or(Error::InvalidIndex)?;
        if raffle.unique_winners {
            idx = resolve_unique_winner(env, seed, i as u32, total_tickets, &winners, idx);
            winning_ticket_ids.set(i, idx);
        }
        let winner = get_ticket_owner(env, idx + 1).ok_or(Error::TicketNotFound)?;
        winners.push_back(winner.clone());
        WinnerDrawn {
            winner,
            ticket_id: idx,
            tier_index: i,
            timestamp: env.ledger().timestamp(),
        }
        .publish(env);
    }

    let mut claimed_winners = Vec::new(env);
    for _ in 0..raffle.prizes.len() {
        claimed_winners.push_back(false);
    }

    env.storage().persistent().set(
        &DataKey::RandomnessSeed,
        &FairnessMetadata {
            seed,
            randomness_source: raffle.randomness_source.clone(),
            winning_ticket_indices: winning_ticket_ids.clone(),
            draw_timestamp: env.ledger().timestamp(),
            draw_sequence: env.ledger().sequence(),
            unique_winners: raffle.unique_winners,
        },
    );

    let winner_addresses: Vec<Address> = winners.iter().map(|w| w.address.clone()).collect();
    raffle.status = RaffleStatus::Finalized;
    raffle.winners = winners;
    raffle.finalized_at = Some(env.ledger().timestamp());
    write_raffle(env, &raffle);

    env.storage().instance().remove(&DataKey::RandomnessRequested);
    env.storage().instance().remove(&DataKey::RandomnessRequestId);
    env.storage().instance().remove(&DataKey::RandomnessRequestLedger);
    env.storage().instance().set(&DataKey::DrawingLock, &false);

    RaffleFinalized {
        raffle_id: env.current_contract_address(),
        winners,
        winning_ticket_ids,
        total_tickets_sold: raffle.tickets_sold,
        randomness_source: raffle.randomness_source.clone(),
        randomness_type,
        finalized_at: env.ledger().timestamp(),
        unique_winners: raffle.unique_winners,
    }
    .publish(env);

    record_leaderboard(env, &raffle);

    Ok(())
}

fn record_leaderboard(env: &Env, raffle: &Raffle) {
    let factory: Address = match env.storage().instance().get(&DataKey::Factory) {
        Some(f) => f,
        None => return,
    };
    let raffle_id = env.current_contract_address();
    let tickets = raffle.tickets_sold as i128;
    let volume = raffle.ticket_price.saturating_mul(tickets);
    let args: Vec<Val> = (
        raffle_id.clone(),
        tickets,
        raffle.prize_amount,
        volume,
    )
        .into_val(env);

    use soroban_sdk::auth::{ContractContext, SubContractInvocation};
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: factory.clone(),
                fn_name: Symbol::new(env, "record_leaderboard_entry"),
                args: args.clone(),
            },
            sub_invocations: Vec::new(env),
        }),
    ]);
    let _ = env.invoke_contract::<()>(
        &factory,
        &Symbol::new(env, "record_leaderboard_entry"),
        args,
    );
}
