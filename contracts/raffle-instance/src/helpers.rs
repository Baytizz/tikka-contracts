//! Shared internal helpers used across every module in the raffle-instance
//! contract.
//!
//! Nothing in this module is part of the public ABI — all items are
//! `pub(crate)`.  They exist to avoid duplicating common logic (storage I/O,
//! state-machine transitions, reentrancy guarding, prize arithmetic) across
//! `init`, `tickets`, `draw`, `admin`, and `claim`.
//!
//! ## Reentrancy guard
//!
//! [`Guard`] is the primary mechanism for preventing reentrancy attacks on
//! token-transfer paths.  It uses an RAII pattern: [`Guard::new`] sets the
//! [`DataKey::ReentrancyGuard`] flag (returning [`Error::Reentrancy`] if
//! already set), and [`Drop`] automatically clears it when the guard goes out
//! of scope.
//!
//! ## Drawing lock
//!
//! [`transition_to_drawing`] both changes the raffle status and sets
//! [`DataKey::DrawingLock`] in a single atomic step.  This prevents
//! concurrent callers from entering the drawing state twice in the same
//! ledger.  The lock is cleared by [`do_finalize_with_seed`] on successful
//! finalization, or by error paths in `draw.rs`.

use soroban_sdk::{token, Address, BytesN, Env, Vec};

use crate::events::{RaffleFinalized, RaffleStatusChanged, WinnerDrawn};
use crate::randomness::{OracleSeedWinnerSelection, WinnerSelectionStrategy};
use crate::{
    DataKey, Error, FairnessMetadata, Raffle, RaffleStatus, RandomnessType, Ticket,
};

use raffle_shared::constants::{
    INSTANCE_TTL_BUMP_LEDGERS, INSTANCE_TTL_THRESHOLD_LEDGERS,
    PERSISTENT_TTL_BUMP_LEDGERS, PERSISTENT_TTL_THRESHOLD_LEDGERS,
};

/// Opportunistically extend TTLs for the instance entry and the key set of
/// persistent entries that cover tickets sold so far.
///
/// Called from hot paths (`buy_tickets`, `do_finalize_with_seed`) and the
/// permissionless `extend_ttl` entrypoint so anyone can keep a raffle alive.
///
/// - Instance storage (Raffle blob, oracle keys, locks): bumped to
///   `INSTANCE_TTL_BUMP_LEDGERS` when below `INSTANCE_TTL_THRESHOLD_LEDGERS`.
/// - Critical persistent keys (TicketBuyers, RandomnessSeed): same policy.
/// - Individual `Ticket` / `TicketCount` / `OwnerTickets` slots: iterated over
///   `tickets_sold` tickets and bumped only when the threshold is breached,
///   so gas cost scales linearly but avoids writes when already fresh.
pub(crate) fn bump_raffle_ttl(env: &Env, tickets_sold: u32) {
    // Instance entry covers all instance-storage keys atomically.
    env.storage().instance().extend_ttl(
        INSTANCE_TTL_THRESHOLD_LEDGERS,
        INSTANCE_TTL_BUMP_LEDGERS,
    );

    // Persistent entries that must outlive the instance TTL.
    macro_rules! bump_persistent {
        ($key:expr) => {
            if env.storage().persistent().has(&$key) {
                env.storage().persistent().extend_ttl(
                    &$key,
                    PERSISTENT_TTL_THRESHOLD_LEDGERS,
                    PERSISTENT_TTL_BUMP_LEDGERS,
                );
            }
        };
    }

    bump_persistent!(DataKey::TicketBuyers);
    bump_persistent!(DataKey::RandomnessSeed);

    // Per-buyer address-keyed persistent slots.
    // Read TicketBuyers once (if present) and bump the per-address indexes.
    if let Some(buyers) = env
        .storage()
        .persistent()
        .get::<_, soroban_sdk::Vec<Address>>(&DataKey::TicketBuyers)
    {
        for buyer in buyers.iter() {
            bump_persistent!(DataKey::TicketCount(buyer.clone()));
            bump_persistent!(DataKey::OwnerTickets(buyer.clone()));
        }
    }

    // Per-ticket persistent slots — only iterate up to tickets_sold.
    for id in 1..=tickets_sold {
        bump_persistent!(DataKey::Ticket(id));
        // CommitEntry may or may not exist; the macro skips absent keys.
        bump_persistent!(DataKey::CommitEntry(id));
    }
}

/// Read the [`Raffle`] struct from instance storage.
///
/// # Errors
///
/// - [`Error::NotInitialized`] — the raffle key is missing (contract not yet
///   initialised via [`init`](crate::init::init)).
pub(crate) fn read_raffle(env: &Env) -> Result<Raffle, Error> {
    env.storage().instance().get(&DataKey::Raffle).ok_or(Error::NotInitialized)
}

/// Write `raffle` back to instance storage, overwriting the previous value.
pub(crate) fn write_raffle(env: &Env, raffle: &Raffle) {
    env.storage().instance().set(&DataKey::Raffle, raffle);
}

/// Load the admin address from persistent storage and enforce `require_auth`.
///
/// # Errors
///
/// - [`Error::NotAuthorized`] — admin key is absent or the caller's
///   authorisation entry does not match.
pub(crate) fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env.storage().persistent().get(&DataKey::Admin).ok_or(Error::NotAuthorized)?;
    admin.require_auth();
    Ok(admin)
}

/// Return the owner address of ticket `ticket_id`, or `None` if the ticket
/// does not exist.
pub(crate) fn get_ticket_owner(env: &Env, ticket_id: u32) -> Option<Address> {
    env.storage().persistent().get::<_, Ticket>(&DataKey::Ticket(ticket_id)).map(|t| t.owner)
}

/// Set the reentrancy guard flag.
///
/// Returns [`Error::Reentrancy`] immediately if the flag is already set,
/// preventing recursive re-entry into protected code paths (e.g., token
/// transfers).  Should be paired with [`release_guard`] or, preferably,
/// wrapped in a [`Guard`] for automatic release.
pub(crate) fn acquire_guard(env: &Env) -> Result<(), Error> {
    if env.storage().instance().has(&DataKey::ReentrancyGuard) {
        return Err(Error::Reentrancy);
    }
    env.storage().instance().set(&DataKey::ReentrancyGuard, &true);
    Ok(())
}

/// Clear the reentrancy guard flag.
///
/// Must be called (or the enclosing [`Guard`] dropped) after every successful
/// or failed protected operation to avoid permanently locking the contract.
pub(crate) fn release_guard(env: &Env) {
    env.storage().instance().remove(&DataKey::ReentrancyGuard);
}

/// RAII reentrancy guard that automatically releases on drop.
///
/// Create with [`Guard::new`]; the guard is released when it goes out of
/// scope, whether the protected block returns normally or via an early `?`.
///
/// ```rust,ignore
/// let _guard = Guard::new(&env)?;
/// // ... token transfers / state mutations ...
/// // guard released here
/// ```
pub(crate) struct Guard<'a> {
    env: &'a Env,
}

impl<'a> Guard<'a> {
    /// Acquire the reentrancy guard.
    ///
    /// # Errors
    ///
    /// - [`Error::Reentrancy`] — the guard is already held by a caller
    ///   higher in the call stack.
    pub(crate) fn new(env: &'a Env) -> Result<Self, Error> {
        acquire_guard(env)?;
        Ok(Guard { env })
    }
}

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        release_guard(self.env);
    }
}

/// Validate slippage and deadline constraints for token-swap paths.
///
/// Currently `#[allow(dead_code)]` — the swap flow is defined but not yet
/// active in the default configuration.  Enabled when the `swap_router` field
/// is populated.
///
/// # Parameters
///
/// - `amount_out` — Actual output amount received from the swap.
/// - `min_amount_out` — Minimum acceptable output (slippage tolerance).
///
/// # Errors
///
/// - [`Error::DeadlinePassed`] — the current ledger timestamp exceeds
///   `raffle.swap_deadline_seconds` added to itself (the deadline is derived
///   from the configurable `swap_deadline_seconds` field).
/// - [`Error::SlippageExceeded`] — `amount_out < min_amount_out`.
#[allow(dead_code)]
pub(crate) fn enforce_swap_guard(
    env: &Env, raffle: &Raffle, amount_out: i128, min_amount_out: i128,
) -> Result<(), Error> {
    let deadline = env.ledger().timestamp() + raffle.swap_deadline_seconds;
    if env.ledger().timestamp() > deadline {
        return Err(Error::DeadlinePassed);
    }
    if amount_out < min_amount_out {
        return Err(Error::SlippageExceeded);
    }
    Ok(())
}

/// Generate a unique oracle randomness `request_id` and record the request in
/// instance storage.
///
/// The `request_id` is the first 8 bytes (big-endian `u64`) of the SHA-256
/// hash of `(ledger_timestamp, ledger_sequence, contract_address)` XDR-packed
/// together.  This makes the ID globally unique across raffles, requests, and
/// ledgers while remaining deterministic for the calling transaction.
///
/// Stores:
/// - [`DataKey::RandomnessRequested`] = `true`
/// - [`DataKey::RandomnessRequestLedger`] = current ledger sequence
/// - [`DataKey::RandomnessRequestId`] = computed `request_id`
///
/// # Errors
///
/// - [`Error::RandomnessAlreadyRequested`] — a request is already in flight;
///   cannot request again until the current one is resolved.
pub(crate) fn request_randomness(env: &Env) -> Result<u64, Error> {
    let already: bool = env.storage().instance().get(&DataKey::RandomnessRequested).unwrap_or(false);
    if already { return Err(Error::RandomnessAlreadyRequested); }

    use soroban_sdk::xdr::ToXdr;
    let request_id_xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address().to_xdr(env),
    ).to_xdr(env);
    let request_id_hash: BytesN<32> = env.crypto().sha256(&request_id_xdr).into();
    let arr = request_id_hash.to_array();
    let mut id_bytes = [0u8; 8];
    id_bytes.copy_from_slice(&arr[..8]);
    let request_id = u64::from_be_bytes(id_bytes);

    env.storage().instance().set(&DataKey::RandomnessRequested, &true);
    env.storage().instance().set(&DataKey::RandomnessRequestLedger, &env.ledger().sequence());
    env.storage().instance().set(&DataKey::RandomnessRequestId, &request_id);
    Ok(request_id)
}

/// Transition the raffle from `Active` to `Drawing` and acquire the drawing
/// lock.
///
/// This is the **single source of truth** for entering the drawing state.
/// It:
///
/// 1. Checks [`DataKey::DrawingLock`] — returns
///    [`Error::DrawingAlreadyInProgress`] if already set, preventing
///    concurrent callers from double-entering.
/// 2. Verifies the raffle is `Active`; returns
///    [`Error::DrawingAlreadyInProgress`] if already `Drawing` or
///    [`Error::InvalidStatusForDrawingTransition`] for any other status.
/// 3. Writes `status = Drawing` and emits [`events::RaffleStatusChanged`].
/// 4. Sets [`DataKey::DrawingLock`] = `true`.
///
/// The lock is cleared by [`do_finalize_with_seed`] on successful
/// finalization or by error recovery paths in `draw.rs`.
///
/// # Errors
///
/// - [`Error::DrawingAlreadyInProgress`] — lock is set or status is already
///   `Drawing`.
/// - [`Error::InvalidStatusForDrawingTransition`] — raffle is in a terminal
///   state (`Finalized`, `Cancelled`, `Failed`, `Claimed`).
pub(crate) fn transition_to_drawing(env: &Env, raffle: &mut Raffle, timestamp: u64) -> Result<(), Error> {
    let drawing_lock: bool = env.storage().instance().get(&DataKey::DrawingLock).unwrap_or(false);
    if drawing_lock { return Err(Error::DrawingAlreadyInProgress); }

    if raffle.status != RaffleStatus::Active {
        if raffle.status == RaffleStatus::Drawing { return Err(Error::DrawingAlreadyInProgress); }
        return Err(Error::InvalidStatusForDrawingTransition);
    }

    let old_status = raffle.status.clone();
    raffle.status = RaffleStatus::Drawing;
    write_raffle(env, raffle);
    RaffleStatusChanged { old_status, new_status: RaffleStatus::Drawing, timestamp }.publish(env);
    env.storage().instance().set(&DataKey::DrawingLock, &true);
    Ok(())
}

/// Return [`Error::ContractPaused`] if the instance-level pause flag is set.
///
/// The pause flag is stored in instance storage under [`DataKey::Paused`] and
/// is set/cleared by the factory relay via the `pause` / `unpause` entry
/// points.
///
/// # Errors
///
/// - [`Error::ContractPaused`] — `DataKey::Paused` is `true`.
pub(crate) fn require_not_paused(env: &Env) -> Result<(), Error> {
    if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {
        return Err(Error::ContractPaused);
    }
    Ok(())
}

/// Verify that `token_address` is a valid Stellar Asset Contract (SAC) by
/// calling `try_decimals` on its token client.
///
/// A non-SAC address will fail the `try_decimals` invocation, which is
/// intercepted and mapped to [`Error::InvalidTokenAddress`].  This prevents
/// misconfigured raffles where the payment token is set to a non-token
/// contract address.
///
/// # Errors
///
/// - [`Error::InvalidTokenAddress`] — the address does not implement the SAC
///   token interface.
pub(crate) fn validate_token_address(env: &Env, token_address: &Address) -> Result<(), Error> {
    let token_client = token::Client::new(env, token_address);
    let _ = token_client.try_decimals().map_err(|_| Error::InvalidTokenAddress)?;
    Ok(())
}

/// Build a `u64` seed from the current ledger state for use when an external
/// oracle has timed out and the internal fallback path is taken.
///
/// The seed is the first 8 bytes (big-endian `u64`) of the SHA-256 hash of
/// `(ledger_timestamp, ledger_sequence, contract_address)` XDR-packed
/// together.  It is **not** the same construction as
/// [`build_internal_seed`](crate::randomness::build_internal_seed) (which
/// returns a `BytesN<32>` for `env.prng().seed()`); this function returns a
/// `u64` directly for use as the seed argument to
/// [`do_finalize_with_seed`].
///
/// Like the PRNG seed, this value is deterministic and visible on-chain and
/// therefore unsuitable for high-stakes draws.  It is used exclusively by the
/// [`RandomnessType::Fallback`] path when the preferred oracle did not respond
/// in time.
pub(crate) fn build_internal_seed_u64(env: &Env) -> u64 {
    use soroban_sdk::xdr::ToXdr;
    let xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address(),
    ).to_xdr(env);
    let hash: BytesN<32> = env.crypto().sha256(&xdr).into();
    let arr = hash.to_array();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&arr[..8]);
    u64::from_be_bytes(bytes)
}

/// Calculate the gross prize amount for winner at position `tier_index`.
///
/// Prize tiers are stored as basis-point allocations in `raffle.prizes`.  The
/// last tier receives the **remainder** of `prize_amount` after all preceding
/// tiers have been allocated, ensuring the total always sums to exactly
/// `prize_amount` regardless of rounding.
///
/// # Parameters
///
/// - `tier_index` — Zero-based index into `raffle.prizes`.  Must be
///   `< raffle.prizes.len()`.
///
/// # Returns
///
/// The gross prize amount in the payment token's base units.
///
/// # Errors
///
/// - [`Error::InvalidIndex`] — `tier_index` is out of range or a basis-point
///   entry is missing.
/// - [`Error::ArithmeticOverflow`] — `prize_amount * bp` or the running
///   `allocated` sum overflows `i128`.
pub(crate) fn calculate_tier_prize(raffle: &Raffle, tier_index: u32) -> Result<i128, Error> {
    let last_tier_index = raffle.prizes.len() - 1;
    if tier_index == last_tier_index {
        let mut allocated = 0i128;
        for i in 0..last_tier_index {
            let bp = raffle.prizes.get(i).ok_or(Error::InvalidIndex)?;
            let amt = raffle.prize_amount.checked_mul(bp as i128).ok_or(Error::ArithmeticOverflow)? / 10000;
            allocated = allocated.checked_add(amt).ok_or(Error::ArithmeticOverflow)?;
        }
        return raffle.prize_amount.checked_sub(allocated).ok_or(Error::ArithmeticOverflow);
    }
    let bp = raffle.prizes.get(tier_index).ok_or(Error::InvalidIndex)?;
    raffle.prize_amount.checked_mul(bp as i128).ok_or(Error::ArithmeticOverflow).map(|a| a / 10000)
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
    env: &Env, mut raffle: Raffle, seed: u64, randomness_type: RandomnessType,
) -> Result<(), Error> {
    let total_tickets = raffle.tickets_sold;
    if total_tickets == 0 { return Err(Error::NoTicketsSold); }
    if raffle.prizes.len() > total_tickets { return Err(Error::MorePrizesThanTickets); }
    if raffle.tickets_sold == 0 { return Err(Error::NoActiveTickets); }

    let selector = OracleSeedWinnerSelection::new(seed);
    let winning_ticket_ids = selector.select_winner_indices(env, total_tickets, raffle.prizes.len());
    let mut winners = Vec::new(env);

    for i in 0..winning_ticket_ids.len() {
        let idx = winning_ticket_ids.get(i).ok_or(Error::InvalidIndex)?;
        let winner = get_ticket_owner(env, idx + 1).ok_or(Error::TicketNotFound)?;
        winners.push_back(winner.clone());
        WinnerDrawn { winner, ticket_id: idx, tier_index: i, timestamp: env.ledger().timestamp() }.publish(env);
    }

    let mut claimed_winners = Vec::new(env);
    for _ in 0..raffle.prizes.len() { claimed_winners.push_back(false); }

    env.storage().persistent().set(&DataKey::RandomnessSeed, &FairnessMetadata {
        seed,
        randomness_source: raffle.randomness_source.clone(),
        winning_ticket_indices: winning_ticket_ids.clone(),
        draw_timestamp: env.ledger().timestamp(),
        draw_sequence: env.ledger().sequence(),
    });

    raffle.status = RaffleStatus::Finalized;
    raffle.winners = winners.clone();
    raffle.claimed_winners = claimed_winners;
    raffle.finalized_at = Some(env.ledger().timestamp());
    write_raffle(env, &raffle);

    env.storage().instance().remove(&DataKey::RandomnessRequested);
    env.storage().instance().remove(&DataKey::RandomnessRequestId);
    env.storage().instance().remove(&DataKey::RandomnessRequestLedger);
    env.storage().instance().set(&DataKey::DrawingLock, &false);

    RaffleFinalized {
        raffle_id: env.current_contract_address(),
        winners, winning_ticket_ids,
        total_tickets_sold: raffle.tickets_sold,
        randomness_source: raffle.randomness_source.clone(),
        randomness_type,
        finalized_at: env.ledger().timestamp(),
    }.publish(env);

    bump_raffle_ttl(env, total_tickets);
    Ok(())
}
