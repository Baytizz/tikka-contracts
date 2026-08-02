//! Fuzz target: commit-reveal randomness with adversarial sequencing
//!
//! Exercises arbitrary interleavings of ticket purchases, hash commitments
//! (honest `sha256(preimage)` and adversarial arbitrary hashes), reveals
//! (correct and wrong preimage), replays, and out-of-window submissions across
//! multiple participants, against a pure-Rust model of the raffle-instance
//! `RandomnessSource::CommitReveal` path (`submit_commit` + the seed
//! aggregation performed by `finalize_raffle`).
//!
//! The seed formula mirrors the contract: accepted commit hashes for tickets
//! `1..=tickets_sold` are concatenated in ticket order and the first 8 bytes of
//! `sha256(concatenation)` become the draw seed (big-endian `u64`).  With zero
//! accepted commits the contract falls back to an internal PRNG seed — modeled
//! here as a deterministic function of the ledger snapshot.
//!
//! # Invariants checked on every execution
//!
//! 1. **Only valid entropy influences the seed** — a commit that is rejected
//!    (out-of-window status, non-owner caller, or nonexistent ticket) never
//!    enters commit storage and therefore never changes the draw seed.
//! 2. **Determinism** — finalizing the same commit state (and ledger snapshot)
//!    always yields the same seed; recomputing the seed from storage after
//!    finalization reproduces the recorded seed.
//! 3. **No sequence panics the model** — every operation returns a `Result`; no
//!    input sequence can trigger an index out-of-bounds, unwrap on `None`, or
//!    any other panic.
//! 4. **Out-of-window submissions are rejected** — commits attempted after
//!    finalization (or after failure) return `InvalidStatus` and leave the
//!    commit set and seed untouched.
//! 5. **Replays overwrite** — re-committing a ticket atomically replaces its
//!    hash; only the last accepted value contributes to the seed, and a fresh
//!    commit invalidates any prior reveal for that ticket.
//! 6. **Wrong-preimage reveals are flagged invalid** — a reveal whose preimage
//!    does not hash to the committed value is recorded as `Invalid` and can
//!    never be treated as valid entropy.
//! 7. **Seed reproducible from valid reveals** — once every committed ticket
//!    has been honestly revealed, the seed recomputed from the revealed
//!    preimages matches the finalized seed.
//!
//! # Running (nightly + cargo-fuzz)
//!
//! ```bash
//! cargo fuzz run fuzz_commit_reveal -- -max_total_time=1800
//! ```

#![no_main]

use std::collections::HashMap;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use sha2::{Digest, Sha256};

// ═══════════════════════════════════════════════════════════════════════════
// State-machine model
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Active,
    Drawing,
    Finalized,
    Failed,
}

/// Honesty status of a ticket's reveal (the off-chain reveal phase).
/// The absence of an entry means no reveal has been attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RevealStatus {
    /// Preimage hashes to the committed value — honest entropy.
    Valid { preimage: [u8; 32] },
    /// Preimage does not match the committed value — cheating detected.
    Invalid,
}

/// Result type returned by model operations; mirrors contract `Error`s.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelError {
    RaffleExpired,
    SoldOut,
    InvalidStatus,
    TicketNotFound,
    NotAuthorized,
    InvalidStateTransition,
    NoCommit,
}

/// Outcome of a reveal attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RevealOutcome {
    Valid,
    WrongPreimage,
}

/// Outcome of a finalize attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FinalizeOutcome {
    Failed,
    Finalized { seed: u64 },
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Extract the first 8 bytes as a big-endian `u64`, exactly like the contract
/// (`seed_bytes.copy_from_slice(&arr[..8]); u64::from_be_bytes(seed_bytes)`).
fn first8_be(hash: &[u8; 32]) -> u64 {
    let mut seed_bytes = [0u8; 8];
    seed_bytes.copy_from_slice(&hash[..8]);
    u64::from_be_bytes(seed_bytes)
}

/// Lightweight pure-Rust model of the CommitReveal raffle path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CommitRevealModel {
    status: Status,
    no_deadline: bool,
    end_time: u64,
    max_tickets: u32,
    min_tickets: u32,
    tickets_sold: u32,
    /// ticket_id → owner participant index
    tickets: HashMap<u32, u32>,
    /// ticket_id → accepted committed hash (only accepted commits)
    commits: HashMap<u32, [u8; 32]>,
    /// ticket_id → honest preimage of the last *honest* commit (off-chain
    /// bookkeeping)
    preimages: HashMap<u32, [u8; 32]>,
    /// ticket_id → reveal honesty status
    reveals: HashMap<u32, RevealStatus>,
    /// Recorded seed once finalised, plus the ledger snapshot it used.
    finalized_seed: Option<u64>,
    finalized_now: Option<u64>,
    /// Stable per-model anchor so the fallback PRNG is deterministic.
    nonce: u64,
}

impl CommitRevealModel {
    fn new(
        max_tickets: u32,
        min_tickets: u32,
        no_deadline: bool,
        end_time: u64,
        nonce: u64,
    ) -> Self {
        CommitRevealModel {
            status: Status::Active,
            no_deadline,
            end_time,
            max_tickets: max_tickets.max(1),
            min_tickets: min_tickets.max(1),
            tickets_sold: 0,
            tickets: HashMap::new(),
            commits: HashMap::new(),
            preimages: HashMap::new(),
            reveals: HashMap::new(),
            finalized_seed: None,
            finalized_now: None,
            nonce,
        }
    }

    /// Attempt to purchase one ticket for `participant` at ledger time `now`.
    fn buy(&mut self, participant: u32, now: u64) -> Result<u32, ModelError> {
        if self.status != Status::Active {
            return Err(ModelError::SoldOut);
        }
        if !self.no_deadline && now >= self.end_time {
            return Err(ModelError::RaffleExpired);
        }
        if self.tickets_sold >= self.max_tickets {
            return Err(ModelError::SoldOut);
        }

        self.tickets_sold += 1;
        let ticket_id = self.tickets_sold; // 1-indexed, matches contract
        self.tickets.insert(ticket_id, participant);

        // Auto-transition to Drawing when sold out (mirrors contract behaviour).
        if self.tickets_sold >= self.max_tickets {
            self.status = Status::Drawing;
        }
        Ok(ticket_id)
    }

    /// Mirrors `submit_commit`: only Active/Drawing status, existing ticket,
    /// and authorization from the current owner.
    ///
    /// `preimage` is `Some` when this is an honest commit whose hash is
    /// `sha256(preimage)`; the honest preimage is bookkept so later reveals
    /// can be replayed.  An `Arbitrary` commit (preimage `None`) stores the raw
    /// hash and discards any recorded preimage.
    fn commit(
        &mut self,
        ticket_id: u32,
        caller: u32,
        hash: [u8; 32],
        honest_preimage: Option<[u8; 32]>,
    ) -> Result<(), ModelError> {
        if self.status != Status::Active && self.status != Status::Drawing {
            return Err(ModelError::InvalidStatus);
        }
        let owner = match self.tickets.get(&ticket_id) {
            Some(owner) => *owner,
            None => return Err(ModelError::TicketNotFound),
        };
        if owner != caller {
            return Err(ModelError::NotAuthorized);
        }

        self.commits.insert(ticket_id, hash);
        match honest_preimage {
            Some(preimage) => {
                self.preimages.insert(ticket_id, preimage);
            }
            None => {
                self.preimages.remove(&ticket_id);
            }
        }
        // A fresh commit invalidates any prior reveal for this ticket.
        self.reveals.remove(&ticket_id);
        Ok(())
    }

    /// Off-chain reveal: verify a preimage against the committed hash.
    fn reveal(&mut self, ticket_id: u32, preimage: [u8; 32]) -> Result<RevealOutcome, ModelError> {
        if !self.tickets.contains_key(&ticket_id) {
            return Err(ModelError::TicketNotFound);
        }
        let Some(hash) = self.commits.get(&ticket_id).copied() else {
            return Err(ModelError::NoCommit);
        };
        if sha256(&preimage) == hash {
            self.reveals
                .insert(ticket_id, RevealStatus::Valid { preimage });
            Ok(RevealOutcome::Valid)
        } else {
            self.reveals.insert(ticket_id, RevealStatus::Invalid);
            Ok(RevealOutcome::WrongPreimage)
        }
    }

    /// Mirrors `finalize_raffle` for the CommitReveal path.
    fn finalize(&mut self, now: u64) -> Result<FinalizeOutcome, ModelError> {
        if self.status != Status::Active && self.status != Status::Drawing {
            return Err(ModelError::InvalidStatus);
        }
        let time_ended = !self.no_deadline && now >= self.end_time;
        let tickets_full = self.tickets_sold >= self.max_tickets;
        if self.status == Status::Active && !time_ended && !tickets_full {
            return Err(ModelError::InvalidStateTransition);
        }
        if self.tickets_sold == 0 || self.tickets_sold < self.min_tickets {
            self.status = Status::Failed;
            return Ok(FinalizeOutcome::Failed);
        }

        let seed = match self.commit_entropy() {
            Some(combined) => first8_be(&sha256(&combined)),
            None => self.fallback_seed(now),
        };
        self.status = Status::Finalized;
        self.finalized_seed = Some(seed);
        self.finalized_now = Some(now);
        Ok(FinalizeOutcome::Finalized { seed })
    }

    /// Concatenated accepted commit hashes in ticket order (contract formula).
    fn commit_entropy(&self) -> Option<Vec<u8>> {
        let mut combined = Vec::new();
        for ticket_id in 1..=self.tickets_sold {
            if let Some(hash) = self.commits.get(&ticket_id) {
                combined.extend_from_slice(hash);
            }
        }
        if combined.is_empty() {
            None
        } else {
            Some(combined)
        }
    }

    /// Deterministic "internal PRNG fallback" — mirrors `build_internal_seed`
    /// being a pure function of the ledger snapshot and raffle state.
    fn fallback_seed(&self, now: u64) -> u64 {
        let mut input = [0u8; 24];
        input[0..8].copy_from_slice(&self.nonce.to_be_bytes());
        input[8..16].copy_from_slice(&(self.tickets_sold as u64).to_be_bytes());
        input[16..24].copy_from_slice(&now.to_be_bytes());
        first8_be(&sha256(&input))
    }

    /// Recompute the draw seed from current storage (determinism check).
    fn recompute_seed(&self, now: u64) -> u64 {
        match self.commit_entropy() {
            Some(combined) => first8_be(&sha256(&combined)),
            None => self.fallback_seed(now),
        }
    }

    /// Recompute the seed from honestly revealed preimages only (INV-7).
    /// Returns `None` while any committed ticket lacks an honest reveal.
    fn seed_from_valid_reveals(&self) -> Option<u64> {
        let mut combined = Vec::new();
        for ticket_id in 1..=self.tickets_sold {
            match self.commits.get(&ticket_id) {
                Some(hash) => match self.reveals.get(&ticket_id) {
                    Some(RevealStatus::Valid { preimage }) => {
                        assert_eq!(
                            sha256(preimage),
                            *hash,
                            "valid reveal preimage must hash to the committed value"
                        );
                        combined.extend_from_slice(hash);
                    }
                    _ => return None,
                },
                None => {}
            }
        }
        if combined.is_empty() {
            None
        } else {
            Some(first8_be(&sha256(&combined)))
        }
    }

    fn assert_invariants(&self) {
        // Structural: ticket bookkeeping is internally consistent.
        assert_eq!(
            self.tickets.len(),
            self.tickets_sold as usize,
            "tickets map size != tickets_sold"
        );

        // Every commit/reveal belongs to a purchased ticket.
        for tid in self.commits.keys() {
            assert!(
                self.tickets.contains_key(tid),
                "commit for ticket {tid} that was never purchased"
            );
        }
        for tid in self.reveals.keys() {
            assert!(
                self.tickets.contains_key(tid),
                "reveal for ticket {tid} that was never purchased"
            );
        }

        // A reveal is only ever recorded for a committed ticket.
        for tid in self.reveals.keys() {
            assert!(
                self.commits.contains_key(tid),
                "reveal recorded for ticket {tid} without a commit"
            );
        }

        // INV-2: finalized seed is reproducible from storage using the snapshot
        // that produced it.
        if let Some(now) = self.finalized_now {
            let seed = self.finalized_seed.expect("finalized_now ⇒ seed");
            assert_eq!(
                self.recompute_seed(now),
                seed,
                "seed not reproducible from storage"
            );
        }
        assert_eq!(
            self.status == Status::Finalized,
            self.finalized_seed.is_some(),
            "finalized_seed present iff status is Finalized"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Fuzzer input types
// ═══════════════════════════════════════════════════════════════════════════

/// How a commit hash is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
enum CommitKind {
    /// `hash = sha256(bytes)`; `bytes` is bookkept as an honest preimage.
    Honest,
    /// `hash = bytes` (adversarial "commit to junk" — no valid reveal exists).
    Arbitrary,
}

/// One operation in the interleaved sequence.
#[derive(Debug, Arbitrary)]
enum Op {
    /// A participant (`participant % NUM_PARTICIPANTS`) buys one ticket.
    Buy { participant: u8, now: u64 },
    /// Attempt to commit `bytes` (honest or arbitrary) for `ticket_id`.
    Commit {
        ticket_id: u8,
        caller: u8,
        kind: CommitKind,
        bytes: [u8; 32],
    },
    /// Attempt to reveal a preimage for `ticket_id`.  When `use_known` is true,
    /// the model replays the ticket's bookkept honest preimage (guaranteeing a
    /// valid reveal whenever the current commit is that honest hash); otherwise
    /// the fuzzer-supplied `preimage` is used (usually a wrong preimage).
    Reveal {
        ticket_id: u8,
        use_known: bool,
        preimage: [u8; 32],
    },
    /// Attempt to finalize the raffle at ledger time `now`.
    Finalize { now: u64 },
}

/// Top-level fuzz input: raffle parameters + operation sequence.
#[derive(Debug, Arbitrary)]
struct FuzzInput {
    /// Max number of tickets (clamped to 1..=32 for fast runs).
    max_tickets_raw: u8,
    /// Minimum tickets (clamped to 1..=max_tickets).
    min_tickets_raw: u8,
    /// Raffle has no deadline.
    no_deadline: bool,
    /// Raffle end_time; ignored when `no_deadline` is true.
    end_time: u64,
    /// Sequence of operations to interleave.
    /// Capped to 128 entries to bound run time.
    ops: Vec<Op>,
}

// Number of distinct simulated participants.
const NUM_PARTICIPANTS: u32 = 8;
// Maximum ops executed per fuzz run to keep execution bounded.
const MAX_OPS: usize = 128;

// ═══════════════════════════════════════════════════════════════════════════
// Fuzz entry point
// ═══════════════════════════════════════════════════════════════════════════

fuzz_target!(|input: FuzzInput| {
    let max_tickets = (input.max_tickets_raw as u32 % 32).max(1);
    let min_tickets = (input.min_tickets_raw as u32 % max_tickets).max(1);

    let mut model = CommitRevealModel::new(
        max_tickets,
        min_tickets,
        input.no_deadline,
        input.end_time,
        0x5449_434B_0000_0000,
    );

    for op in input.ops.iter().take(MAX_OPS) {
        let before = model.clone();
        let mut used_reveal_preimage: Option<[u8; 32]> = None;
        let mut reveal_outcome: Option<RevealOutcome> = None;
        let result = match op {
            Op::Buy { participant, now } => model
                .buy((*participant as u32) % NUM_PARTICIPANTS, *now)
                .map(|_| ()),
            Op::Commit {
                ticket_id,
                caller,
                kind,
                bytes,
            } => {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                let caller = (*caller as u32) % NUM_PARTICIPANTS;
                let (hash, preimage) = match kind {
                    CommitKind::Honest => {
                        let preimage = *bytes;
                        (sha256(&preimage), Some(preimage))
                    }
                    CommitKind::Arbitrary => (*bytes, None),
                };
                model.commit(tid, caller, hash, preimage)
            }
            Op::Reveal {
                ticket_id,
                use_known,
                preimage,
            } => {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                let used = if *use_known {
                    model.preimages.get(&tid).copied().unwrap_or(*preimage)
                } else {
                    *preimage
                };
                used_reveal_preimage = Some(used);
                match model.reveal(tid, used) {
                    Ok(outcome) => {
                        reveal_outcome = Some(outcome);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            Op::Finalize { now } => model.finalize(*now).map(|_| ()),
        };

        let failed = result.is_err();
        match (op, result) {
            (Op::Buy { now, .. }, Ok(())) => {
                // INV-3: a successful buy is in range and inside the window.
                let tid = model.tickets_sold;
                assert!(tid >= 1 && tid <= max_tickets, "ticket_id out of range");
                assert!(
                    model.no_deadline || *now < model.end_time,
                    "buy succeeded past the deadline"
                );
            }
            (Op::Buy { now, .. }, Err(ModelError::RaffleExpired)) => {
                assert!(
                    !model.no_deadline && *now >= model.end_time,
                    "RaffleExpired fired but the deadline was not reached"
                );
            }
            (Op::Buy { .. }, Err(ModelError::SoldOut)) => {
                assert!(
                    model.tickets_sold >= max_tickets || model.status != Status::Active,
                    "SoldOut fired but capacity was not reached"
                );
            }
            (_, Err(ModelError::SoldOut)) => {
                unreachable!("SoldOut is only returned by buy")
            }

            (Op::Commit { ticket_id, .. }, Ok(())) => {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                // INV-5: an accepted commit only ever mutates the target ticket
                // (it may overwrite with the same hash — a no-op replay).
                let touched_elsewhere: Vec<u32> = before
                    .commits
                    .iter()
                    .filter(|(k, _)| **k != tid)
                    .filter(|(k, v)| model.commits.get(k) != Some(v))
                    .map(|(k, _)| *k)
                    .collect();
                assert!(
                    touched_elsewhere.is_empty(),
                    "accepted commit touched {touched_elsewhere:?} besides {tid}"
                );
                assert!(
                    model
                        .commits
                        .keys()
                        .all(|k| *k == tid || before.commits.contains_key(k)),
                    "accepted commit added an unexpected ticket"
                );
                assert!(
                    model.commits.contains_key(&tid),
                    "accepted commit missing for ticket {tid}"
                );
                assert_eq!(
                    model.reveals.get(&tid),
                    None,
                    "a fresh commit must invalidate prior reveals"
                );
                // Accepted commit implies an owned, in-window ticket.
                assert!(model.tickets.contains_key(&tid));
                assert!(
                    model.status == Status::Active || model.status == Status::Drawing,
                    "commit accepted outside Active/Drawing"
                );
            }
            (Op::Commit { .. }, Err(ModelError::InvalidStatus)) => {
                assert!(
                    model.status == Status::Finalized || model.status == Status::Failed,
                    "InvalidStatus for a commit outside terminal states"
                );
            }
            (Op::Commit { ticket_id, .. }, Err(ModelError::TicketNotFound)) => {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                assert!(
                    !model.tickets.contains_key(&tid),
                    "TicketNotFound for an existing ticket"
                );
            }
            (
                Op::Commit {
                    ticket_id, caller, ..
                },
                Err(ModelError::NotAuthorized),
            ) => {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                assert!(model.tickets.contains_key(&tid));
                assert_ne!(
                    *model.tickets.get(&tid).expect("ticket exists"),
                    (*caller as u32) % NUM_PARTICIPANTS,
                    "NotAuthorized for the ticket owner"
                );
            }

            (Op::Reveal { ticket_id, .. }, Ok(()))
                if reveal_outcome == Some(RevealOutcome::Valid) =>
            {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                let used = used_reveal_preimage.expect("reveal always sets the used preimage");
                // INV-6: a valid reveal's preimage hashes to the committed value.
                assert_eq!(
                    sha256(&used),
                    *model.commits.get(&tid).expect("committed ticket"),
                    "Valid reveal whose preimage does not match the commit"
                );
                assert!(
                    matches!(
                        model.reveals.get(&tid),
                        Some(RevealStatus::Valid { preimage }) if *preimage == used
                    ),
                    "Valid reveal not recorded with the used preimage"
                );
            }
            (Op::Reveal { ticket_id, .. }, Ok(()))
                if reveal_outcome == Some(RevealOutcome::WrongPreimage) =>
            {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                let used = used_reveal_preimage.expect("reveal always sets the used preimage");
                // INV-6: a wrong-preimage reveal is flagged invalid and its
                // preimage does NOT hash to the committed value.
                assert_ne!(
                    sha256(&used),
                    *model.commits.get(&tid).expect("committed ticket"),
                    "WrongPreimage reveal whose preimage matches the commit"
                );
                assert_eq!(
                    model.reveals.get(&tid),
                    Some(&RevealStatus::Invalid),
                    "WrongPreimage reveal not flagged invalid"
                );
            }
            (Op::Reveal { .. }, Ok(())) => {
                unreachable!("a successful reveal is always Valid or WrongPreimage")
            }
            (Op::Reveal { ticket_id, .. }, Err(ModelError::TicketNotFound)) => {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                assert!(
                    !model.tickets.contains_key(&tid),
                    "reveal TicketNotFound for an existing ticket"
                );
            }
            (Op::Reveal { ticket_id, .. }, Err(ModelError::NoCommit)) => {
                let tid = ((*ticket_id as u32) % max_tickets).max(1);
                assert!(model.tickets.contains_key(&tid));
                assert!(
                    !model.commits.contains_key(&tid),
                    "NoCommit for a committed ticket"
                );
            }

            (Op::Finalize { now }, Ok(_)) => match model.finalized_seed {
                Some(seed) => {
                    // INV-2: determinism — recompute with the same snapshot.
                    assert_eq!(
                        model.recompute_seed(model.finalized_now.unwrap_or(*now)),
                        seed,
                        "finalized seed not reproducible"
                    );
                    // INV-7: when every committed ticket is honestly revealed,
                    // the seed is reproducible from the revealed preimages.
                    if let Some(repro) = model.seed_from_valid_reveals() {
                        assert_eq!(repro, seed, "seed differs from valid-reveal recompute");
                    }
                }
                None => {
                    assert_eq!(
                        model.status,
                        Status::Failed,
                        "finalize without a seed must be Failed"
                    );
                    assert_eq!(model.finalized_seed, None);
                }
            },
            (Op::Finalize { now: _ }, Err(ModelError::InvalidStatus)) => {
                assert!(
                    model.status == Status::Finalized || model.status == Status::Failed,
                    "double-finalize returned InvalidStatus"
                );
            }
            (Op::Finalize { now }, Err(ModelError::InvalidStateTransition)) => {
                assert_eq!(model.status, Status::Active);
                assert!(
                    model.no_deadline || *now < model.end_time,
                    "InvalidStateTransition despite time_ended"
                );
                assert!(
                    model.tickets_sold < max_tickets,
                    "InvalidStateTransition despite tickets_full"
                );
            }

            (op, Err(e)) => panic!("unexpected {e:?} from {op:?}"),
        }

        // INV-1 & INV-4: any rejected operation must leave the model untouched.
        if failed {
            assert_eq!(model, before, "rejected op changed the model state");
        }

        model.assert_invariants();
    }

    model.assert_invariants();
});

// ═══════════════════════════════════════════════════════════════════════════
// Smoke tests (cargo test -p raffle-fuzz)
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    fn new_model() -> CommitRevealModel {
        CommitRevealModel::new(3, 1, true, 0, 0x5449_434B_0000_0000)
    }

    fn buy_all(model: &mut CommitRevealModel) -> Vec<u32> {
        let mut ids = Vec::new();
        for p in 0..3 {
            ids.push(model.buy(p, 100).unwrap());
        }
        ids
    }

    // ── buy ─────────────────────────────────────────────────────────────────

    #[test]
    fn buy_assigns_sequential_ids() {
        let mut m = new_model();
        assert_eq!(m.buy(0, 100), Ok(1));
        assert_eq!(m.buy(5, 100), Ok(2));
        assert_eq!(m.tickets_sold, 2);
        m.assert_invariants();
    }

    #[test]
    fn buy_sold_out_when_full() {
        let mut m = new_model();
        buy_all(&mut m);
        assert_eq!(m.status, Status::Drawing);
        assert_eq!(m.buy(9, 100), Err(ModelError::SoldOut));
        m.assert_invariants();
    }

    #[test]
    fn buy_rejects_after_deadline() {
        let mut m = CommitRevealModel::new(5, 1, false, 1000, 1);
        assert_eq!(m.buy(0, 1000), Err(ModelError::RaffleExpired));
        m.assert_invariants();
    }

    #[test]
    fn buy_no_deadline_never_expires() {
        let mut m = CommitRevealModel::new(1, 1, true, 0, 1);
        assert_eq!(m.buy(0, u64::MAX), Ok(1));
        m.assert_invariants();
    }

    // ── commit ──────────────────────────────────────────────────────────────

    #[test]
    fn commit_requires_ticket_ownership() {
        let mut m = new_model();
        buy_all(&mut m);
        assert_eq!(
            m.commit(1, 7, [1u8; 32], None),
            Err(ModelError::NotAuthorized)
        );
        assert!(m.commits.is_empty());
        m.assert_invariants();
    }

    #[test]
    fn commit_requires_existing_ticket() {
        let mut m = new_model();
        buy_all(&mut m);
        assert_eq!(
            m.commit(99, 0, [1u8; 32], None),
            Err(ModelError::TicketNotFound)
        );
        m.assert_invariants();
    }

    #[test]
    fn honest_commit_records_preimage() {
        let mut m = new_model();
        buy_all(&mut m);
        let preimage = [7u8; 32];
        let hash = sha256(&preimage);
        m.commit(1, 0, hash, Some(preimage)).unwrap();
        assert_eq!(m.commits.get(&1), Some(&hash));
        assert_eq!(m.preimages.get(&1), Some(&preimage));
        m.assert_invariants();
    }

    #[test]
    fn commit_out_of_window_is_rejected() {
        let mut m = new_model();
        buy_all(&mut m);
        m.finalize(100).unwrap();
        assert_eq!(
            m.commit(1, 0, [1u8; 32], None),
            Err(ModelError::InvalidStatus)
        );
        m.assert_invariants();
    }

    #[test]
    fn replay_overwrites_hash_and_invalidates_reveal() {
        let mut m = new_model();
        buy_all(&mut m);
        let p1 = [1u8; 32];
        let p2 = [2u8; 32];
        m.commit(1, 0, sha256(&p1), Some(p1)).unwrap();
        m.reveal(1, p1).unwrap();
        m.commit(1, 0, sha256(&p2), Some(p2)).unwrap();
        // INV-5: only the last hash survives and the old reveal is gone.
        assert_eq!(m.commits.get(&1), Some(&sha256(&p2)));
        assert_eq!(m.reveals.get(&1), None);
        assert_eq!(m.preimages.get(&1), Some(&p2));
        m.assert_invariants();
    }

    // ── reveal ──────────────────────────────────────────────────────────────

    #[test]
    fn correct_preimage_reveals_valid() {
        let mut m = new_model();
        buy_all(&mut m);
        let p = [9u8; 32];
        m.commit(1, 0, sha256(&p), Some(p)).unwrap();
        assert_eq!(m.reveal(1, p), Ok(RevealOutcome::Valid));
        assert_eq!(
            m.reveals.get(&1),
            Some(&RevealStatus::Valid { preimage: p })
        );
        m.assert_invariants();
    }

    #[test]
    fn wrong_preimage_reveals_invalid() {
        let mut m = new_model();
        buy_all(&mut m);
        let p = [9u8; 32];
        m.commit(1, 0, sha256(&p), Some(p)).unwrap();
        assert_eq!(m.reveal(1, [0u8; 32]), Ok(RevealOutcome::WrongPreimage));
        assert_eq!(m.reveals.get(&1), Some(&RevealStatus::Invalid));
        m.assert_invariants();
    }

    #[test]
    fn reveal_without_commit_is_no_commit() {
        let mut m = new_model();
        buy_all(&mut m);
        assert_eq!(m.reveal(1, [1u8; 32]), Err(ModelError::NoCommit));
        m.assert_invariants();
    }

    // ── finalize / seed ──────────────────────────────────────────────────────

    #[test]
    fn finalize_requires_sufficient_tickets() {
        let mut m = CommitRevealModel::new(5, 4, false, 100, 1);
        m.buy(0, 50).unwrap();
        m.buy(1, 50).unwrap();
        m.buy(2, 50).unwrap();
        // Active + not ended + not full → transition rejected.
        assert_eq!(m.finalize(50), Err(ModelError::InvalidStateTransition));
        // Time ended but fewer than `min_tickets` sold → Failed, no seed.
        assert_eq!(m.finalize(100), Ok(FinalizeOutcome::Failed));
        assert_eq!(m.status, Status::Failed);
        assert_eq!(m.finalized_seed, None);
        m.assert_invariants();
    }

    #[test]
    fn finalize_active_without_deadline_or_full_is_rejected() {
        let mut m = CommitRevealModel::new(5, 1, false, 1000, 1);
        m.buy(0, 100).unwrap();
        assert_eq!(m.finalize(100), Err(ModelError::InvalidStateTransition));
        m.assert_invariants();
    }

    #[test]
    fn double_finalize_is_rejected() {
        let mut m = new_model();
        buy_all(&mut m);
        m.finalize(100).unwrap();
        assert_eq!(m.finalize(101), Err(ModelError::InvalidStatus));
        m.assert_invariants();
    }

    #[test]
    fn seed_is_sha256_of_concatenated_commits() {
        let mut m = new_model();
        buy_all(&mut m);
        let hashes: Vec<[u8; 32]> = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        for (tid, h) in hashes.iter().enumerate() {
            m.commit(tid as u32 + 1, tid as u32, *h, None).unwrap();
        }
        let Ok(FinalizeOutcome::Finalized { seed }) = m.finalize(100) else {
            panic!("expected finalized");
        };
        let mut combined = Vec::new();
        for h in &hashes {
            combined.extend_from_slice(h);
        }
        assert_eq!(seed, first8_be(&sha256(&combined)));
        m.assert_invariants();
    }

    #[test]
    fn seed_deterministic_across_equal_commits() {
        let mut m = new_model();
        buy_all(&mut m);
        for tid in 1..=3 {
            m.commit(tid, tid - 1, [tid as u8; 32], None).unwrap();
        }
        // INV-2: two identical pre-finalize states finalize to the same seed,
        // and the seed is reproducible from storage after finalization.
        let mut a = m.clone();
        let mut b = m.clone();
        let FinalizeOutcome::Finalized { seed: s1 } = a.finalize(100).unwrap() else {
            panic!("expected finalized");
        };
        let FinalizeOutcome::Finalized { seed: s2 } = b.finalize(100).unwrap() else {
            panic!("expected finalized");
        };
        assert_eq!(
            s1, s2,
            "identical pre-finalize states must yield the same seed"
        );
        assert_eq!(a.recompute_seed(100), s1);
        assert_eq!(b.recompute_seed(100), s2);
        a.assert_invariants();
        b.assert_invariants();
    }

    #[test]
    fn rejected_commit_never_changes_seed() {
        let mut m = new_model();
        buy_all(&mut m);
        for tid in 1..=3 {
            m.commit(tid, tid - 1, [tid as u8; 32], None).unwrap();
        }
        let before = m.clone();
        // Rejected: non-owner, nonexistent ticket, out-of-window.
        assert_eq!(
            m.commit(1, 7, [0u8; 32], None),
            Err(ModelError::NotAuthorized)
        );
        assert_eq!(
            m.commit(99, 0, [0u8; 32], None),
            Err(ModelError::TicketNotFound)
        );
        m.finalize(100).unwrap();
        let seed = m.finalized_seed.unwrap();
        assert_eq!(
            m.commit(1, 0, [0u8; 32], None),
            Err(ModelError::InvalidStatus)
        );
        // INV-1/INV-4: none of the rejected commits touched commit storage.
        assert_eq!(m.commits, before.commits);
        assert_eq!(m.finalized_seed, Some(seed));
        m.assert_invariants();
    }

    #[test]
    fn zero_commits_falls_back_to_deterministic_internal_seed() {
        let mut m = new_model();
        buy_all(&mut m);
        let Ok(FinalizeOutcome::Finalized { seed }) = m.finalize(100) else {
            panic!("expected finalized");
        };
        assert_eq!(seed, m.fallback_seed(100), "fallback must be deterministic");
        m.assert_invariants();
    }

    #[test]
    fn seed_reproducible_from_valid_reveals() {
        let mut m = new_model();
        buy_all(&mut m);
        let preimages: Vec<[u8; 32]> = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        for (tid, p) in preimages.iter().enumerate() {
            m.commit(tid as u32 + 1, tid as u32, sha256(p), Some(*p))
                .unwrap();
        }
        for tid in 1..=3 {
            m.reveal(tid, preimages[(tid - 1) as usize]).unwrap();
        }
        let Ok(FinalizeOutcome::Finalized { seed }) = m.finalize(100) else {
            panic!("expected finalized");
        };
        // INV-7: with every ticket honestly revealed the seed is reproducible.
        assert_eq!(m.seed_from_valid_reveals(), Some(seed));
        m.assert_invariants();
    }

    #[test]
    fn partial_reveals_cannot_reproduce_seed() {
        let mut m = new_model();
        buy_all(&mut m);
        let preimages: Vec<[u8; 32]> = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        for (tid, p) in preimages.iter().enumerate() {
            m.commit(tid as u32 + 1, tid as u32, sha256(p), Some(*p))
                .unwrap();
        }
        m.reveal(1, preimages[0]).unwrap();
        m.reveal(2, [0xff; 32]).unwrap(); // wrong preimage
        m.finalize(100).unwrap();
        assert_eq!(
            m.seed_from_valid_reveals(),
            None,
            "a wrong-preimage reveal must break full reproducibility"
        );
        m.assert_invariants();
    }
}
