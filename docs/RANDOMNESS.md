# Randomness Modes: Internal vs External vs CommitReveal

Tikka raffles select winners using one of three `RandomnessSource` values (`contracts/raffle-shared/src/lib.rs`). The mode is fixed in `RaffleConfig` at creation and enforced by `finalize_raffle` / `provide_randomness` (`contracts/raffle-instance/src/draw.rs`, `randomness.rs`).

## Quick decision table

| Mode | Trust assumption | Who can influence outcome? | Extra cost / ops | Recommended prize scale |
|---|---|---|---|---|
| **Internal** | Honest-enough validators + unpredictable timing | Finalizer timing; validators biasing ledger timestamp/sequence | Lowest — single finalize tx | Low-stakes (README guide: **≲ ~500 XLM** prize) |
| **External** | Honest oracle key + live oracle service | Oracle (bounded by Ed25519 proof over request); timeout fallback | Oracle hosting + callback tx; possible fallback tx | Medium / high-stakes |
| **CommitReveal** | Enough buyers submit unpredictable commits | Buyers who commit; last-mover / withholding risks; zero-commit → Internal fallback | Per-ticket `submit_commit` txs | Medium-stakes when buyers are engaged |
| **Quorum** | At least 1 honest oracle out of k-of-n delivered | Single oracle cannot bias outcome; requires k-of-n collusion to manipulate | Multi-oracle hosting + k callback txs | High-stakes / large treasuries |


If you need protocol detail for commits, also read [COMMIT_REVEAL.md](COMMIT_REVEAL.md).

---

## 1. Internal PRNG (`RandomnessSource::Internal = 0`)

### How the seed is built

Primary helper: `build_internal_seed` / `PrngWinnerSelection` in `randomness.rs`, and `build_internal_seed_u64` used by the finalize path in `lib.rs` / `draw.rs`.

Entropy mixed into the 32-byte path:

1. Ledger timestamp  
1. Ledger sequence  
1. Network id (SHA-256 of network passphrase)  
1. Raffle contract address  
1. `tickets_sold` (folded into the PRNG seed bytes)

Values are XDR-packed and hashed with `env.crypto().sha256`, then fed to `env.prng().seed(...)`. Winner indices are sampled without replacement via `u64_in_range`.

The compact u64 seed used when finalizing through `do_finalize_with_seed` hashes `(timestamp, sequence, current_contract_address)` and takes the first 8 bytes.

### Who can influence it

- Anyone who can choose **when** `finalize_raffle` lands can work from visible ledger state.
- Validators can influence timestamp/sequence.
- Outcomes are **deterministic** for identical ledger + raffle inputs (good for audit, bad against motivated bias).

### Timeout / fallback

None. Finalize completes in the same call once tickets meet `min_tickets`.

### Cost

One successful `finalize_raffle` (plus prior ticket txs). No oracle.

### When to use

Low-stakes community raffles, demos, and tests. **Do not** rely on Internal for large treasuries or adversarial settings.

---

## 2. External oracle (`RandomnessSource::External = 1`)

### How the seed is built

1. `finalize_raffle` transitions to `Drawing`, sets `DrawingLock`, and calls `request_randomness`.
1. Contract stores `RandomnessRequested`, `RandomnessRequestLedger`, and a `RandomnessRequestId` derived from `(timestamp, sequence, contract_address)` via SHA-256 → first 8 bytes.
1. Emits `RandomnessRequested` for the off-chain `oracle/` service.
1. Oracle calls `provide_randomness(random_seed, public_key, proof, request_id)`.
1. Contract verifies Ed25519 over `build_vrf_proof_message` = XDR`(contract_address, request_id, random_seed)`.
1. `OracleSeedWinnerSelection` maps the seed to winner indices with rejection sampling (no modulo bias).

`Fairness` / seed metadata is stored under `DataKey::RandomnessSeed` (persistent).

### Who can influence it

- Only the configured `oracle_address` can auth the callback.
- The oracle chooses `random_seed` but must present a valid signature bound to `request_id` and the raffle contract.
- If the oracle never answers, creator/admin may call `trigger_randomness_fallback` after the timeout.

### Timeout / fallback (`ORACLE_TIMEOUT_LEDGERS = 200`)

Defined in `raffle-shared::constants` and the instance crate (~**200 ledgers ≈ 17 minutes** at 5s/ledger).

After `request_ledger + 200`:

| `trigger_randomness_fallback(..., do_refund)` | Result |
|---|---|
| `do_refund = true` | Status → `Cancelled` (`CancelReason::OracleTimeout`); request keys cleared; lock released |
| `do_refund = false` | Finalize with **Internal** u64 seed and `RandomnessType::Fallback`; emits `RandomnessFallbackTriggered` |

Calling fallback **before** the timeout returns `Error::FallbackTooEarly` (9).

### Cost

- Oracle process (Node 20, see `oracle/README.md`)
- Extra callback transaction
- Possible fallback transaction if the oracle is down

### When to use

High-stakes draws, public prize pools, or any case where Internal bias is unacceptable. Requires a reliable oracle key and monitoring.

---

## 3. CommitReveal (`RandomnessSource::CommitReveal = 2`)

### How the seed is built

1. During `Active`, ticket owners call `submit_commit(ticket_id, hash)` with `hash = sha256(secret)`.
1. Entries stored as persistent `CommitEntry(ticket_id)` → `{ committer, hash }` (ticket-keyed so transfers keep entropy; see [COMMIT_REVEAL.md](COMMIT_REVEAL.md)).
1. On `finalize_raffle`, contract concatenates all present commit hashes in ticket-id order, SHA-256s the blob, and uses the **first 8 bytes** as a `u64` seed.
1. Finalize proceeds via `do_finalize_with_seed` with `RandomnessType::Prng`.

### Who can influence it

- Each committer contributes preimage entropy (if they later reveal off-chain).
- Parties who **withhold** commits reduce entropy.
- If **zero** commits exist at finalize, the contract **falls back to Internal PRNG** (same path as Internal after the CommitReveal branch).

### Timeout / fallback

No oracle timeout. Fallback is immediate at finalize when `commits_found == 0`.

### Cost

One `submit_commit` per participating ticket (user-paid) plus finalize. No oracle host required.

### When to use

Medium-stakes raffles where buyers can be asked to commit, and you want stronger bias resistance than Internal without operating an oracle. Educate users to commit; otherwise you silently degrade to Internal.

---

## 4. Quorum-of-oracles randomness (`RandomnessSource::Quorum { k, oracles }`)

### How the seed is built

1. On draw initiation (`finalize_raffle` or ticket sales complete), the contract transitions to `Drawing` and emits `RandomnessRequested` events for all $n$ registered oracle addresses (`request fan-out`).
2. Each oracle submits its randomness via `provide_randomness(random_seed, public_key, proof, request_id)`.
3. The contract verifies the Ed25519 proof, matches `public_key` to a registered oracle in `oracles`, calls `oracle.require_auth()`, and enforces per-oracle deduplication (`duplicate submissions rejected`).
4. Delivered seeds are accumulated on-chain under `DataKey::QuorumSeeds` and `DataKey::QuorumOraclesSubmitted`.
5. Once at least $k$ unique registered oracles have submitted valid seeds, the contract aggregates all delivered seeds via SHA-256 over their concatenated big-endian bytes (`aggregate_quorum_seeds`) to form the final 64-bit seed.
6. The raffle is finalized via `do_finalize_with_seed` using the aggregated VRF seed.


## K-of-N Quorum Randomness Scheme

To eliminate single-oracle trust assumptions in high-stakes raffles, the contract supports a `Quorum` randomness mode.

### Architecture & Protocol Steps

1. **Request Fan-out**: When a draw is initiated, a `Quorum { k, oracles }` configuration specifies the threshold `k` and the set of $n$ authorized oracle addresses (`Vec<Address>`).
2. **Per-Oracle Deduplication**: Each registered oracle can submit its seed via `provide_randomness(env, caller, seed)`. The contract tracks delivered seeds in storage using an `AddressSet` / map indexed by oracle address. Duplicate submissions from the same oracle are rejected with `Error::DuplicateOracleSubmission`.
3. **Aggregation Function**: The aggregated seed is accumulated iteratively as seeds arrive using bitwise XOR and SHA-256 hashing:
   $$\text{Aggregated Seed} = \text{SHA-256}(\text{Accumulated Seed} \oplus \text{Oracle Seed})$$
   Once $k$ unique valid oracle submissions are delivered, the state transitions to `Ready` and the draw can be executed.
4. **Timeout & Fallback**: If $k$ oracles fail to submit seeds within `ORACLE_TIMEOUT_LEDGERS` ledgers from the draw request height, any caller can trigger a fallback mechanism (e.g., falling back to commit-reveal or admin fallback seed depending on protocol fallback policy).

### Who can influence it

- No single oracle alone can bias or predict the outcome.
- As long as at least 1 of the $k$ delivered seeds comes from an honest oracle, the output of the SHA-256 aggregation function is cryptographically uniform and un-biasable.
- Collusion among at least $k$ oracles is required to manipulate or predict the outcome.

### Timeout / fallback (`ORACLE_TIMEOUT_LEDGERS = 200`)

If fewer than $k$ oracles deliver valid seeds before `request_ledger + 200`:

| `trigger_randomness_fallback(..., do_refund)` | Result |
|---|---|
| `do_refund = true` | Status → `Cancelled` (`CancelReason::OracleTimeout`); clears request & quorum state |
| `do_refund = false` | Finalize with **Internal** u64 seed and `RandomnessType::Fallback` |

---


## Guidance thresholds

These are **policy recommendations** aligned with README / code comments — not on-chain enforced limits:

| Prize / risk profile | Suggested mode |
|---|---|
| Demo, tiny rewards, trusted community (≲ ~500 XLM) | **Internal** |
| Meaningful value, engaged ticket buyers | **CommitReveal** (+ document commit UX) |
| Large prizes, public adversarial setting, institutional | **External** (+ monitored oracle, tested fallback) |

Also consider:

- Can you run `oracle/` with `ORACLE_SECRET_KEY` secured? If no → avoid External.
- Will most tickets call `submit_commit`? If no → CommitReveal ≈ Internal at finalize.
- Is validator/finalizer collusion in-scope? If yes → External (or CommitReveal with high commit participation).

---

## Failure modes summary

| Mode | Primary failure mode | Protocol response |
|---|---|---|
| Internal | Biased finalize timing | None (inherent) |
| External | Oracle silent | After 200 ledgers: refund cancel **or** Internal fallback |
| External | Wrong `request_id` / bad proof | Tx rejects (`InvalidParameters` / crypto fail) |
| CommitReveal | No commits | Internal PRNG fallback |
| Any | `tickets_sold < min_tickets` or zero sold | `Failed` + `RaffleFailed` (no draw) |
| Any | Concurrent finalize | `DrawingLock` → `DrawingAlreadyInProgress` |

---

## Code map

| Concern | Location |
|---|---|
| Enum | `contracts/raffle-shared/src/lib.rs` → `RandomnessSource` |
| Timeout constant | `contracts/raffle-shared/src/constants.rs` → `ORACLE_TIMEOUT_LEDGERS` |
| Seed + strategies | `contracts/raffle-instance/src/randomness.rs` |
| Finalize / oracle / fallback | `contracts/raffle-instance/src/draw.rs` |
| Commits | `contracts/raffle-instance/src/tickets.rs` → `submit_commit` |
| Off-chain oracle | `oracle/` |

## Related docs

- [COMMIT_REVEAL.md](COMMIT_REVEAL.md) — commit/reveal protocol details  
- [STORAGE.md](STORAGE.md) — randomness-related keys and tiers  
- [ARCHITECTURE.md](ARCHITECTURE.md) — factory → instance → oracle flow  
- [EVENTS.md](EVENTS.md) — `RandomnessRequested`, `RandomnessReceived`, fallback events  
