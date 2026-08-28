# Raffle Fuzz Testing Suite

Cargo-fuzz harness for issues #86, #466, and #632 — fuzz targets covering
`buy_ticket`, `finalize_raffle`, winner selection, the refund/cancellation
surface, and commit-reveal randomness sequencing.

---

## Prerequisites

| Tool | How to install |
|------|----------------|
| Rust **nightly** | `rustup toolchain install nightly` |
| cargo-fuzz | `cargo install cargo-fuzz` |
| Linux / WSL | Required by `cargo-fuzz` (uses LLVM libFuzzer) |

---

## Targets

| Target name | Contract entrypoint | What is fuzzed |
|---|---|---|
| `fuzz_buy_ticket` | `buy_ticket` | Deadline guard, sold-out cap, single-ticket policy, tickets_sold increment |
| `fuzz_finalize_raffle` | `finalize_raffle` + `provide_randomness` | Winner-index in-bounds invariant (internal & external randomness paths) |
| `fuzz_winner_selection` | `OracleSeedWinnerSelection` | Index bounds, uniqueness, and termination of the rejection-sampling loop |
| `fuzz_refund_cancel` | `refund_ticket` + `cancel_raffle` | Arbitrary interleavings of buys, cancels (creator/admin/unauthorized), and refunds; see invariant list below |
| `fuzz_commit_reveal` | `submit_commit` + `finalize_raffle` (CommitReveal) | Adversarial sequencing of commits, honest & wrong-preimage reveals, replays, and out-of-window submissions; see invariant list below |

### `fuzz_refund_cancel` invariants

| # | Invariant |
|---|-----------|
| 1 | **No double-refund** — a ticket may only be refunded once |
| 2 | **Total refunded ≤ total paid** — sum of refunds never exceeds revenue collected |
| 3 | **Contract balance ≥ 0** — `total_collected − total_refunded` is always non-negative |
| 4 | **Refunds only in terminal-refundable states** — only `Cancelled` and `Failed` permit refunds |
| 5 | **Terminal states are terminal** — `Cancelled`/`Failed` status never changes |
| 6 | **Cancel is idempotent w.r.t. status** — re-cancelling returns `AlreadyCancelled`, state unchanged |
| 7 | **Only authorised roles may cancel** — `Unauthorized` returns `NotAuthorized`, state unchanged |
| 8 | **tickets_sold ≤ max_tickets** — buy cap is never breached |

### `fuzz_commit_reveal` invariants

The target models the CommitReveal path end-to-end: accepted commits are stored
per ticket (`DataKey::CommitEntry(ticket_id)`), the draw seed is
`u64::from_be_bytes(sha256(commit_1 ‖ … ‖ commit_n)[..8])` over the accepted
commits in ticket order, and a zero-commit finalize falls back to a
deterministic internal PRNG seed.

| # | Invariant |
|---|-----------|
| 1 | **Only valid entropy influences the seed** — a rejected commit (out-of-window status, non-owner caller, or nonexistent ticket) never enters commit storage and never changes the draw seed |
| 2 | **Determinism** — finalizing the same commit state (and ledger snapshot) always yields the same seed; recomputing from storage reproduces the recorded seed |
| 3 | **No sequence panics** — every operation returns a `Result`; no input sequence triggers an index out-of-bounds, unwrap on `None`, or other panic |
| 4 | **Out-of-window submissions are rejected** — commits after finalization/failure return `InvalidStatus` and leave commit state and seed untouched |
| 5 | **Replays overwrite** — re-committing a ticket atomically replaces its hash; only the last accepted value counts, and a fresh commit invalidates any prior reveal |
| 6 | **Wrong-preimage reveals are flagged invalid** — a reveal whose preimage does not hash to the committed value is recorded as `Invalid` |
| 7 | **Seed reproducible from valid reveals** — once every committed ticket is honestly revealed, the seed recomputed from the revealed preimages matches the finalized seed |

---

## Running the Fuzzer (≥ 30 minutes)

From the **repository root**:

```bash
# Switch to nightly once (per repo)
rustup override set nightly

# Buy-ticket target — 30-minute run
cargo fuzz run fuzz_buy_ticket -- -max_total_time=1800

# Finalize-raffle target — 30-minute run
cargo fuzz run fuzz_finalize_raffle -- -max_total_time=1800

# Winner-selection target — 30-minute run
cargo fuzz run fuzz_winner_selection -- -max_total_time=1800

# Refund/cancel target — 30-minute run (issue #466)
cargo fuzz run fuzz_refund_cancel -- -max_total_time=1800

# Commit-reveal target — 30-minute run (issue #632)
cargo fuzz run fuzz_commit_reveal -- -max_total_time=1800
```

`-max_total_time=1800` instructs libFuzzer to stop after 1 800 s (30 min).
A run with no `CRASH` or `panic` in the output satisfies the acceptance criterion.

---

## Cross-platform Smoke Tests (Windows / stable)

A deterministic smoke-test battery is embedded in each fuzz target file and can
be run on **any** platform with stable Rust:

```powershell
cargo test -p raffle-fuzz
```

---

## Reproducing a Crash

If `cargo fuzz run` discovers a crash, it writes a reproduction file to:

```text
fuzz/artifacts/<target-name>/crash-<hash>
```

Reproduce it with:

```bash
cargo fuzz run <target-name> fuzz/artifacts/<target-name>/crash-<hash>
```

### Reproducing a crash from CI

The [nightly fuzz workflow](../.github/workflows/fuzz.yml) uploads crash inputs as
workflow artifacts and opens a GitHub issue with reproduction steps.

1. Open the failing workflow run under **Actions → Fuzz**.
2. Download the artifact named `fuzz-crash-<target>-<run-id>`.
3. Extract the crash file and place it under `fuzz/artifacts/<target>/`, or pass
   the path directly:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz --locked
cargo fuzz run <target-name> path/to/crash-<hash>
```

4. After fixing the bug, minimize the input and commit it as a regression seed:

```bash
cargo fuzz tmin <target-name> fuzz/artifacts/<target-name>/crash-<hash>
cp fuzz/artifacts/<target-name>/crash-<hash> fuzz/corpus/<target-name>/
git add fuzz/corpus/<target-name>/crash-<hash>
```

---

## Corpus

`cargo-fuzz` accumulates interesting inputs in:

```text
fuzz/corpus/<target-name>/
```

Commit this directory to seed future runs and prevent regression.

---

## Acceptance Criteria

- [x] Fuzz target for `buy_ticket` (issue #86)
- [x] Fuzz target for `finalize_raffle` (issue #86)
- [x] Fuzz target for `winner_selection` (issue #86)
- [x] Fuzz target for `refund_cancel` (issue #466)
- [x] Fuzz target for `commit_reveal` (issue #632)
- [x] Nightly CI job runs all targets with corpus caching (see `.github/workflows/fuzz.yml`)
- [ ] Fuzzer runs for at least 30 minutes without discovery of panics *(run in CI or locally)*
