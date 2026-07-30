//! Read-only query functions for the raffle-instance contract.
//!
//! All functions in this module are pure reads — they never mutate state,
//! emit events, or require authorisation.  They exist to give off-chain
//! clients and other contracts a stable query surface without having to parse
//! raw storage keys.
//!
//! | Function | Returns |
//! |---|---|
//! | [`get_raffle`] | Full [`Raffle`](crate::Raffle) struct |
//! | [`get_fairness_data`] | Post-draw audit data ([`FairnessData`]) |
//! | [`is_paused`] | Whether the instance-level pause flag is set |
//! | [`is_ticket_sales_paused`] | Whether ticket sales are paused within an active raffle |
//! | [`get_accumulated_fees`] | Protocol fees collected but not yet withdrawn |

use soroban_sdk::{Env, Vec};

use raffle_shared::FairnessData;

use crate::{read_raffle, DataKey, Error, FairnessMetadata};

/// Return the full [`Raffle`](crate::Raffle) struct from instance storage.
///
/// This is the primary state-inspection endpoint.  It returns every field of
/// the raffle configuration and runtime state in a single call.
///
/// # Errors
///
/// - [`Error::NotInitialized`] — the contract has not been initialised yet
///   (i.e., [`init`](crate::init::init) has never succeeded).
pub(crate) fn get_raffle(env: Env) -> Result<crate::Raffle, Error> {
    read_raffle(&env)
}

/// Return the post-draw fairness audit data for this raffle.
///
/// [`FairnessData`] contains everything needed for an off-chain observer to
/// independently verify that the winner selection was performed correctly:
///
/// - The seed used for winner selection.
/// - The randomness source that produced it.
/// - The ordered list of ticket IDs that were in scope for the draw.
/// - The winning ticket indices (zero-based offsets into `ticket_ids`).
/// - The ledger timestamp and sequence at draw time.
///
/// The data is stored in **persistent** storage under
/// [`DataKey::RandomnessSeed`] by
/// [`do_finalize_with_seed`](crate::helpers::do_finalize_with_seed) so it
/// survives ledger-entry TTL expiry and remains queryable long after the raffle
/// ends.
///
/// # Errors
///
/// - [`Error::InvalidStatus`] — the draw has not completed yet (no
///   `FairnessMetadata` written to storage).
/// - [`Error::NotInitialized`] — the contract has not been initialised.
///
/// See also: [`docs/RANDOMNESS.md`](../../../../docs/RANDOMNESS.md) — audit
/// and replay verification.
pub(crate) fn get_fairness_data(env: Env) -> Result<FairnessData, Error> {
    let meta: FairnessMetadata = env.storage().persistent().get(&DataKey::RandomnessSeed).ok_or(Error::InvalidStatus)?;
    let raffle = read_raffle(&env)?;
    let mut ticket_ids = Vec::new(&env);
    for i in 1..=raffle.tickets_sold { ticket_ids.push_back(i); }
    Ok(FairnessData {
        seed: meta.seed,
        randomness_source: meta.randomness_source,
        ticket_ids,
        winning_ticket_indices: meta.winning_ticket_indices,
        draw_timestamp: meta.draw_timestamp,
        draw_sequence: meta.draw_sequence,
        unique_winners: meta.unique_winners,
    })
}

/// Return `true` if the instance-level contract pause flag is set.
///
/// When `true`, [`buy_tickets`](crate::tickets::buy_tickets) and
/// [`deposit_prize`](crate::init::deposit_prize) will return
/// [`Error::ContractPaused`].  The flag is set/cleared by the factory relay
/// via the `pause` / `unpause` admin entry points.
pub(crate) fn is_paused(env: Env) -> bool {
    env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
}

/// Return `true` if ticket sales are paused within an otherwise-active raffle.
///
/// This is a finer-grained pause than [`is_paused`]: the raffle remains
/// `Active` and the prize stays locked, but new ticket purchases are rejected.
/// The flag is controlled by the creator or admin via `pause_ticket_sales` /
/// `resume_ticket_sales`.
///
/// Returns `false` if the raffle has not been initialised.
pub(crate) fn is_ticket_sales_paused(env: Env) -> bool {
    read_raffle(&env).map(|r| r.ticket_sales_paused).unwrap_or(false)
}

/// Return the total protocol fees collected in this raffle instance that have
/// not yet been withdrawn.
///
/// Fees accumulate in instance storage under [`DataKey::AccumulatedFees`]
/// on each successful ticket purchase.  They are swept out by the
/// `withdraw_fees` admin function, which requires the raffle to be
/// `Finalized` or `Claimed`.
///
/// Returns `0` before any tickets have been sold or after fees have been
/// fully withdrawn.
pub(crate) fn get_accumulated_fees(env: Env) -> i128 {
    env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0)
}
