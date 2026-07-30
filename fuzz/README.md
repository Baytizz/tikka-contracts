# Raffle Fuzz Testing Suite

Cargo-fuzz harness for issues #86 and #466 — fuzz targets covering `buy_ticket`,
`finalize_raffle`, winner selection, and the refund/cancellation surface.

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
- [ ] Fuzzer runs for at least 30 minutes without discovery of panics *(run in CI or locally)*
