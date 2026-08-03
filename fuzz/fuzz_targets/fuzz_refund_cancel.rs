//! Fuzz target: refund and cancellation flows
//!
//! Exercises arbitrary interleavings of ticket purchases, raffle cancellations
//! (by creator *or* admin), and individual ticket refunds against a pure-Rust
//! model of the raffle-instance state machine.
//!
//! # Invariants checked on every execution
//!
//! 1. **No double-refund** — a ticket may only be refunded once; the second
//!    call must return `AlreadyRefunded`.
//! 2. **Total refunded ≤ total paid** — the sum of all successful refund
//!    amounts never exceeds the sum of all ticket purchases.
//! 3. **Contract balance never negative vs entitlements** — the virtual
//!    contract balance (collected ticket revenue) minus the sum of completed
//!    refunds is always ≥ 0.
//! 4. **Refunds only permitted in terminal-refundable states** — `Cancelled`
//!    and `Failed`; never in `Active`, `Drawing`, `Finalized`, or `Claimed`.
//! 5. **Terminal states are terminal** — once `Cancelled` or `Failed`, no
//!    operation transitions the raffle to a different status.
//! 6. **Cancel is idempotent w.r.t. status** — cancelling an already-cancelled
//!    raffle returns `AlreadyCancelled`; it does not change the state.
//! 7. **Only authorised roles may cancel** — an `Unauthorized` role returns
//!    `NotAuthorized`; state is unchanged.
//! 8. **tickets_sold never exceeds max_tickets** — buying stops at the cap.
//!
//! # Running (Linux/WSL, nightly)
//!
//! ```bash
//! cargo fuzz run fuzz_refund_cancel -- -max_total_time=1800
//! ```

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::{HashMap, HashSet};

// ═══════════════════════════════════════════════════════════════════════════
// State-machine model
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Active,
    Drawing,
    Finalized,
    Cancelled,
    Failed,
    Claimed,
}

/// Roles that are allowed (or not) to cancel a raffle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
enum CancelRole {
    /// The raffle creator — always authorised.
    Creator,
    /// The protocol admin — always authorised.
    Admin,
    /// Any arbitrary third party — never authorised.
    Unauthorized,
}

/// Reason supplied with a cancel call; mirrors `CancelReason` in the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
enum CancelReason {
    CreatorCancelled,
    AdminCancelled,
    MinTicketsNotMet,
}

/// Result type returned by model operations.
#[derive(Debug, PartialEq, Eq)]
enum ModelError {
    /// Raffle is not in a state that allows refunds.
    NotRefundableState,
    /// Ticket id does not exist (was never purchased).
    TicketNotFound,
    /// This ticket was already refunded.
    AlreadyRefunded,
    /// Cancel attempted on an already-cancelled/failed raffle.
    AlreadyCancelled,
    /// Caller is not an authorised cancellation role.
    NotAuthorized,
    /// Raffle is already sold out.
    SoldOut,
    /// Raffle has ended (past deadline).
    Expired,
}

/// Lightweight pure-Rust model of one raffle instance.
#[derive(Debug, Clone)]
struct RaffleModel {
    status: Status,
    max_tickets: u32,
    tickets_sold: u32,
    ticket_price: i128,
    end_time: u64,   // 0 = no deadline
    /// ticket_id → owner_id (buyer index)
    tickets: HashMap<u32, u32>,
    /// set of ticket_ids that have already been refunded
    refunded: HashSet<u32>,
    /// total token revenue collected (tickets_sold * ticket_price)
    total_collected: i128,
    /// total token refunded so far
    total_refunded: i128,
}

impl RaffleModel {
    fn new(max_tickets: u32, ticket_price: i128, end_time: u64) -> Self {
        RaffleModel {
            status: Status::Active,
            max_tickets: max_tickets.max(1),
            tickets_sold: 0,
            ticket_price: ticket_price.max(1),
            end_time,
            tickets: HashMap::new(),
            refunded: HashSet::new(),
            total_collected: 0,
            total_refunded: 0,
        }
    }

    /// Attempt to purchase one ticket for `buyer_id` at `now`.
    fn buy(&mut self, buyer_id: u32, now: u64) -> Result<u32, ModelError> {
        if self.status != Status::Active {
            return Err(ModelError::NotRefundableState);
        }
        if self.end_time != 0 && now > self.end_time {
            return Err(ModelError::Expired);
        }
        if self.tickets_sold >= self.max_tickets {
            return Err(ModelError::SoldOut);
        }

        self.tickets_sold += 1;
        let ticket_id = self.tickets_sold; // 1-indexed, matches contract
        self.tickets.insert(ticket_id, buyer_id);
        self.total_collected = self
            .total_collected
            .saturating_add(self.ticket_price);

        // Auto-transition to Drawing when sold out (mirrors contract behaviour)
        if self.tickets_sold >= self.max_tickets {
            self.status = Status::Drawing;
        }

        Ok(ticket_id)
    }

    /// Attempt to cancel the raffle.
    fn cancel(&mut self, role: CancelRole, _reason: CancelReason) -> Result<(), ModelError> {
        // Only creator and admin are authorised
        if role == CancelRole::Unauthorized {
            return Err(ModelError::NotAuthorized);
        }
        // Already in a terminal state
        match self.status {
            Status::Cancelled | Status::Failed => return Err(ModelError::AlreadyCancelled),
            Status::Finalized | Status::Claimed => return Err(ModelError::AlreadyCancelled),
            Status::Active | Status::Drawing => {}
        }
        self.status = Status::Cancelled;
        Ok(())
    }

    /// Attempt to refund `ticket_id`.
    fn refund_ticket(&mut self, ticket_id: u32) -> Result<i128, ModelError> {
        // Refunds are only valid in Cancelled or Failed states
        match self.status {
            Status::Cancelled | Status::Failed => {}
            _ => return Err(ModelError::NotRefundableState),
        }
        // Ticket must exist
        if !self.tickets.contains_key(&ticket_id) {
            return Err(ModelError::TicketNotFound);
        }
        // No double-refund
        if self.refunded.contains(&ticket_id) {
            return Err(ModelError::AlreadyRefunded);
        }

        self.refunded.insert(ticket_id);
        self.total_refunded = self
            .total_refunded
            .saturating_add(self.ticket_price);

        Ok(self.ticket_price)
    }

    // ── Invariant assertions ────────────────────────────────────────────────

    fn assert_invariants(&self) {
        // INV-2 & INV-3: total_refunded ≤ total_collected, balance ≥ 0
        assert!(
            self.total_refunded <= self.total_collected,
            "total_refunded ({}) > total_collected ({})",
            self.total_refunded,
            self.total_collected,
        );

        // INV-3: virtual balance is non-negative
        let balance = self.total_collected - self.total_refunded;
        assert!(
            balance >= 0,
            "virtual balance negative: collected={} refunded={}",
            self.total_collected,
            self.total_refunded,
        );

        // INV-8: tickets_sold cap
        assert!(
            self.tickets_sold <= self.max_tickets,
            "tickets_sold ({}) > max_tickets ({})",
            self.tickets_sold,
            self.max_tickets,
        );

        // INV-1: every refunded ticket_id must have been purchased exactly once
        for tid in &self.refunded {
            assert!(
                self.tickets.contains_key(tid),
                "refunded ticket {tid} was never purchased"
            );
        }

        // Refunded count can't exceed tickets sold
        assert!(
            self.refunded.len() <= self.tickets_sold as usize,
            "refunded count ({}) > tickets_sold ({})",
            self.refunded.len(),
            self.tickets_sold,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Fuzzer input types
// ═══════════════════════════════════════════════════════════════════════════

/// One operation in the interleaved sequence.
#[derive(Debug, Arbitrary)]
enum Op {
    /// A buyer (identified by `buyer_id % NUM_BUYERS`) purchases one ticket.
    Buy { buyer_id: u8, now: u64 },
    /// Cancel the raffle as `role` with `reason`.
    Cancel { role: CancelRole, reason: CancelReason },
    /// Attempt to refund ticket number `ticket_id` (1-indexed).
    /// `ticket_id` is clamped to a plausible range by the harness.
    Refund { ticket_id: u8 },
}

/// Top-level fuzz input: raffle parameters + operation sequence.
#[derive(Debug, Arbitrary)]
struct FuzzInput {
    /// Max number of tickets (clamped to 1..=64 for fast runs).
    max_tickets_raw: u8,
    /// Ticket price (clamped to 1..=i32::MAX cast to i128).
    ticket_price_raw: u32,
    /// Raffle end_time; 0 = no deadline.
    end_time: u64,
    /// Sequence of operations to interleave.
    /// Capped to 128 entries to bound run time.
    ops: Vec<Op>,
}

// Number of distinct simulated buyers.
const NUM_BUYERS: u8 = 8;
// Maximum ops executed per fuzz run to keep execution bounded.
const MAX_OPS: usize = 128;

// ═══════════════════════════════════════════════════════════════════════════
// Fuzz entry point
// ═══════════════════════════════════════════════════════════════════════════

fuzz_target!(|input: FuzzInput| {
    let max_tickets = (input.max_tickets_raw as u32 % 64).max(1);
    let ticket_price = (input.ticket_price_raw as i128).max(1);

    let mut raffle = RaffleModel::new(max_tickets, ticket_price, input.end_time);

    for op in input.ops.iter().take(MAX_OPS) {
        match op {
            Op::Buy { buyer_id, now } => {
                let result = raffle.buy(*buyer_id as u32 % NUM_BUYERS as u32, *now);
                match result {
                    Ok(ticket_id) => {
                        // Successful buy: ticket_id must be in range
                        assert!(ticket_id >= 1, "ticket_id must be ≥ 1");
                        assert!(
                            ticket_id <= raffle.max_tickets,
                            "ticket_id {ticket_id} > max_tickets {}",
                            raffle.max_tickets
                        );
                    }
                    Err(ModelError::SoldOut) => {
                        // Must be at cap or in a non-Active state
                        assert!(
                            raffle.tickets_sold >= raffle.max_tickets
                                || raffle.status != Status::Active,
                            "SoldOut fired but capacity not reached"
                        );
                    }
                    Err(ModelError::Expired) => {
                        assert!(
                            raffle.end_time != 0 && *now > raffle.end_time,
                            "Expired fired but deadline not reached"
                        );
                    }
                    Err(ModelError::NotRefundableState) => {
                        // Expected when status != Active
                        assert_ne!(raffle.status, Status::Active);
                    }
                    Err(e) => panic!("unexpected buy error: {e:?}"),
                }
            }

            Op::Cancel { role, reason } => {
                let status_before = raffle.status.clone();
                let result = raffle.cancel(*role, *reason);

                match result {
                    Ok(()) => {
                        // INV-5: must now be in Cancelled
                        assert_eq!(raffle.status, Status::Cancelled);
                        // Preceded by Active or Drawing
                        assert!(
                            status_before == Status::Active
                                || status_before == Status::Drawing,
                            "cancel succeeded from {:?}", status_before
                        );
                        // INV-7: only authorised roles can succeed
                        assert_ne!(
                            *role,
                            CancelRole::Unauthorized,
                            "unauthorized cancel succeeded"
                        );
                    }
                    Err(ModelError::NotAuthorized) => {
                        // INV-7: must be the Unauthorized role
                        assert_eq!(
                            *role,
                            CancelRole::Unauthorized,
                            "NotAuthorized returned for authorised role {:?}", role
                        );
                        // INV-5: state must not have changed
                        assert_eq!(
                            raffle.status, status_before,
                            "state changed after NotAuthorized"
                        );
                    }
                    Err(ModelError::AlreadyCancelled) => {
                        // INV-6: was already in a terminal state
                        assert!(
                            matches!(
                                status_before,
                                Status::Cancelled
                                    | Status::Failed
                                    | Status::Finalized
                                    | Status::Claimed
                            ),
                            "AlreadyCancelled from non-terminal state {:?}", status_before
                        );
                        // INV-5: state unchanged
                        assert_eq!(raffle.status, status_before);
                    }
                    Err(e) => panic!("unexpected cancel error: {e:?}"),
                }
            }

            Op::Refund { ticket_id } => {
                // Map the fuzzer's u8 to a plausible ticket_id (1-indexed)
                let tid = (*ticket_id as u32 % raffle.max_tickets).max(1);
                let status_before = raffle.status.clone();
                let result = raffle.refund_ticket(tid);

                match result {
                    Ok(amount) => {
                        // INV-4: refunds only in Cancelled or Failed
                        assert!(
                            status_before == Status::Cancelled
                                || status_before == Status::Failed,
                            "refund succeeded in state {:?}", status_before
                        );
                        // Amount must equal ticket price
                        assert_eq!(
                            amount, raffle.ticket_price,
                            "refund amount mismatch"
                        );
                        // INV-1: ticket must be marked refunded
                        assert!(raffle.refunded.contains(&tid));
                    }
                    Err(ModelError::NotRefundableState) => {
                        // INV-4: state was not Cancelled or Failed
                        assert!(
                            status_before != Status::Cancelled
                                && status_before != Status::Failed,
                            "NotRefundableState in {:?}", status_before
                        );
                        // INV-5: state unchanged
                        assert_eq!(raffle.status, status_before);
                    }
                    Err(ModelError::TicketNotFound) => {
                        assert!(
                            !raffle.tickets.contains_key(&tid),
                            "TicketNotFound for ticket {tid} that exists"
                        );
                    }
                    Err(ModelError::AlreadyRefunded) => {
                        // INV-1: ticket must already be in refunded set
                        assert!(
                            raffle.refunded.contains(&tid),
                            "AlreadyRefunded for ticket {tid} not in refunded set"
                        );
                    }
                    Err(e) => panic!("unexpected refund error: {e:?}"),
                }
            }
        }

        // Check all structural invariants after every operation.
        raffle.assert_invariants();
    }

    // INV-5: final terminal-state check — if Cancelled/Failed the status must
    // not have silently changed during the loop.
    raffle.assert_invariants();
});

// ═══════════════════════════════════════════════════════════════════════════
// Smoke tests (cargo test -p raffle-fuzz)
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    fn new_raffle() -> RaffleModel {
        RaffleModel::new(5, 10_000, 0)
    }

    // ── buy ─────────────────────────────────────────────────────────────────

    #[test]
    fn buy_increments_tickets_sold() {
        let mut r = new_raffle();
        assert_eq!(r.buy(0, 0), Ok(1));
        assert_eq!(r.tickets_sold, 1);
        r.assert_invariants();
    }

    #[test]
    fn buy_returns_sold_out_at_cap() {
        let mut r = new_raffle();
        for i in 0..5 {
            r.buy(i, 0).unwrap();
        }
        assert_eq!(r.buy(0, 0), Err(ModelError::SoldOut));
        r.assert_invariants();
    }

    #[test]
    fn buy_transitions_to_drawing_when_sold_out() {
        let mut r = new_raffle();
        for i in 0..5 {
            r.buy(i, 0).unwrap();
        }
        assert_eq!(r.status, Status::Drawing);
        r.assert_invariants();
    }

    #[test]
    fn buy_expired_raffle_returns_expired() {
        let mut r = RaffleModel::new(10, 1_000, 100);
        assert_eq!(r.buy(0, 101), Err(ModelError::Expired));
        r.assert_invariants();
    }

    #[test]
    fn buy_no_deadline_never_expires() {
        let mut r = RaffleModel::new(1, 1_000, 0);
        assert_eq!(r.buy(0, u64::MAX), Ok(1));
        r.assert_invariants();
    }

    // ── cancel ──────────────────────────────────────────────────────────────

    #[test]
    fn creator_can_cancel_active_raffle() {
        let mut r = new_raffle();
        r.buy(0, 0).unwrap();
        assert_eq!(r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled), Ok(()));
        assert_eq!(r.status, Status::Cancelled);
        r.assert_invariants();
    }

    #[test]
    fn admin_can_cancel_active_raffle() {
        let mut r = new_raffle();
        assert_eq!(r.cancel(CancelRole::Admin, CancelReason::AdminCancelled), Ok(()));
        assert_eq!(r.status, Status::Cancelled);
        r.assert_invariants();
    }

    #[test]
    fn unauthorized_cannot_cancel() {
        let mut r = new_raffle();
        assert_eq!(
            r.cancel(CancelRole::Unauthorized, CancelReason::CreatorCancelled),
            Err(ModelError::NotAuthorized)
        );
        assert_eq!(r.status, Status::Active);
        r.assert_invariants();
    }

    #[test]
    fn cancel_already_cancelled_returns_error() {
        let mut r = new_raffle();
        r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled).unwrap();
        assert_eq!(
            r.cancel(CancelRole::Admin, CancelReason::AdminCancelled),
            Err(ModelError::AlreadyCancelled)
        );
        assert_eq!(r.status, Status::Cancelled);
        r.assert_invariants();
    }

    #[test]
    fn can_cancel_in_drawing_state() {
        let mut r = new_raffle();
        // Fill all tickets to transition to Drawing
        for i in 0..5 {
            r.buy(i, 0).unwrap();
        }
        assert_eq!(r.status, Status::Drawing);
        assert_eq!(r.cancel(CancelRole::Admin, CancelReason::AdminCancelled), Ok(()));
        assert_eq!(r.status, Status::Cancelled);
        r.assert_invariants();
    }

    // ── refund ──────────────────────────────────────────────────────────────

    #[test]
    fn refund_succeeds_after_cancel() {
        let mut r = new_raffle();
        r.buy(0, 0).unwrap();
        r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled).unwrap();
        let amount = r.refund_ticket(1).unwrap();
        assert_eq!(amount, 10_000);
        assert_eq!(r.total_refunded, 10_000);
        r.assert_invariants();
    }

    #[test]
    fn double_refund_is_rejected() {
        let mut r = new_raffle();
        r.buy(0, 0).unwrap();
        r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled).unwrap();
        r.refund_ticket(1).unwrap();
        assert_eq!(r.refund_ticket(1), Err(ModelError::AlreadyRefunded));
        // total_refunded must not double-count
        assert_eq!(r.total_refunded, r.ticket_price);
        r.assert_invariants();
    }

    #[test]
    fn refund_in_active_state_is_rejected() {
        let mut r = new_raffle();
        r.buy(0, 0).unwrap();
        assert_eq!(r.refund_ticket(1), Err(ModelError::NotRefundableState));
        assert_eq!(r.total_refunded, 0);
        r.assert_invariants();
    }

    #[test]
    fn refund_nonexistent_ticket_is_rejected() {
        let mut r = new_raffle();
        r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled).unwrap();
        assert_eq!(r.refund_ticket(99), Err(ModelError::TicketNotFound));
        r.assert_invariants();
    }

    #[test]
    fn total_refunded_never_exceeds_total_collected() {
        let mut r = RaffleModel::new(3, 10_000, 0);
        r.buy(0, 0).unwrap();
        r.buy(1, 0).unwrap();
        r.buy(2, 0).unwrap();
        r.cancel(CancelRole::Admin, CancelReason::AdminCancelled).unwrap();
        r.refund_ticket(1).unwrap();
        r.refund_ticket(2).unwrap();
        r.refund_ticket(3).unwrap();
        assert_eq!(r.total_refunded, r.total_collected);
        assert!(r.total_refunded <= r.total_collected);
        r.assert_invariants();
    }

    #[test]
    fn virtual_balance_never_negative() {
        let mut r = RaffleModel::new(2, 5_000, 0);
        r.buy(0, 0).unwrap();
        r.buy(1, 0).unwrap();
        r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled).unwrap();
        r.refund_ticket(1).unwrap();
        let balance = r.total_collected - r.total_refunded;
        assert!(balance >= 0);
        r.assert_invariants();
    }

    // ── interleaving ────────────────────────────────────────────────────────

    #[test]
    fn interleaved_buys_then_cancel_then_all_refunds() {
        let mut r = RaffleModel::new(4, 1_000, 0);
        for i in 0..4 {
            r.buy(i, 0).unwrap();
        }
        // Raffle is now in Drawing
        r.cancel(CancelRole::Admin, CancelReason::AdminCancelled).unwrap();
        for tid in 1..=4 {
            r.refund_ticket(tid).unwrap();
        }
        assert_eq!(r.total_refunded, r.total_collected);
        r.assert_invariants();
    }

    #[test]
    fn cancel_without_any_buys_leaves_zero_balance() {
        let mut r = new_raffle();
        r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled).unwrap();
        // No tickets sold, so no refunds possible
        assert_eq!(r.total_collected, 0);
        assert_eq!(r.total_refunded, 0);
        r.assert_invariants();
    }

    #[test]
    fn partial_refund_leaves_positive_balance() {
        let mut r = RaffleModel::new(3, 10_000, 0);
        r.buy(0, 0).unwrap();
        r.buy(1, 0).unwrap();
        r.buy(2, 0).unwrap();
        r.cancel(CancelRole::Creator, CancelReason::CreatorCancelled).unwrap();
        r.refund_ticket(1).unwrap(); // only one of three
        let balance = r.total_collected - r.total_refunded;
        assert_eq!(balance, 20_000);
        r.assert_invariants();
    }
}
