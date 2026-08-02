# Draw Attestation Feature Implementation

## Summary

Implemented a comprehensive draw attestation feature that allows third-party auditors to verify raffle draws with a single contract call. This addresses the issue where verifiers previously needed to make multiple queries and trust that separate data points (config, winners, metadata) belonged together.

## Changes Made

### 1. New Module: `contracts/raffle-instance/src/attestation.rs`

Created a dedicated attestation module with:

- **`DrawAttestation` struct**: Contains all verification data in one response:
  - `fairness_data`: Seed, ticket IDs, winning indices, timestamps
  - `metadata_hash`: SHA-256 hash of off-chain metadata
  - `winner_addresses`: Resolved winner addresses in prize-tier order
  - `winning_ticket_ids`: Winning ticket IDs (1-indexed)
  - `randomness_source`: Internal/External/CommitReveal
  - `config_hash`: SHA-256 hash of effective raffle configuration
  - `total_tickets_sold`, `prize_distribution_bp`, `prize_amount`, `ticket_price`

- **`get_draw_attestation()`**: Public function that:
  - Only works in `Finalized` or `Claimed` states
  - Loads fairness metadata from persistent storage
  - Reconstructs complete `FairnessData`
  - Resolves winning ticket IDs from indices
  - Computes configuration hash for verification
  - Returns comprehensive `DrawAttestation` struct

- **`compute_config_hash()`**: Helper that deterministically hashes:
  - `max_tickets`, `ticket_price`, `prize_amount`, `prizes`
  - `randomness_source`, `payment_token`, `creator`

### 2. Modified: `contracts/raffle-instance/src/lib.rs`

- Added `mod attestation;` to module declarations
- Added `DataKey::MetadataHash` to store metadata hash during initialization
- Modified `init()` to store metadata hash in persistent storage:
  ```rust
  env.storage()
      .persistent()
      .set(&DataKey::MetadataHash, &config.metadata_hash);
  ```
- Added public contract function:
  ```rust
  pub fn get_draw_attestation(env: Env) -> Result<attestation::DrawAttestation, Error>
  ```

### 3. Modified: `contracts/raffle-instance/src/views.rs`

Updated module documentation to list the new `get_draw_attestation` function in the read-only query functions table.

### 4. Enhanced Documentation: `docs/RANDOMNESS.md`

Added comprehensive "Independent Draw Verification" section covering:

#### Quick Start Example
Shows how to verify a draw in 4 steps with code snippets.

#### What the Attestation Returns
Full breakdown of the `DrawAttestation` struct fields.

#### Availability & Errors
- Status requirements (Finalized/Claimed only)
- Error handling (InvalidStatus before finalization)
- Storage persistence details

#### Verification Procedure (4 Checks)

1. **Configuration Hash Integrity**
   - Recompute config hash and compare
   - Prevents parameter tampering

2. **Metadata Hash Check**
   - Fetch off-chain metadata
   - Verify SHA-256 matches recorded hash

3. **Winner Selection Reproduction**
   - Use recorded seed to independently reproduce winners
   - Detailed rejection sampling algorithm explanation
   - Matches `OracleSeedWinnerSelection` implementation

4. **Winner-to-Owner Resolution**
   - Verify ticket IDs resolve to claimed addresses
   - Note storage limitations after `wipe_storage`

#### Randomness Source Considerations
Table showing verification strength and trust requirements for:
- External (VRF): Ed25519 signature verification
- CommitReveal: Commit-based entropy tracking
- Internal: Deterministic ledger-based seed
- Fallback: Same as Internal

#### Example: Complete Audit Script
Full Rust function demonstrating end-to-end verification workflow.

#### When to Verify
- Before prize claims
- Post-mortem audits
- Continuous monitoring
- Dispute resolution

#### Limitations
- Ticket ownership after `wipe_storage`
- CommitReveal entropy unpredictability
- Internal/Fallback timing bias

#### Code References
Table mapping components to file locations.

## Acceptance Criteria Met

✅ **Single Contract Call**: Verifier calls `get_draw_attestation()` once to get all required data

✅ **Complete Attestation**: Returns:
- FairnessData (seed, ticket_ids, winning_indices, timestamps)
- Metadata hash
- Winner addresses and ticket IDs
- Randomness source
- Configuration hash

✅ **State Restrictions**: Only available in Finalized/Claimed states, errors otherwise

✅ **Documentation**: Comprehensive verification procedure documented in `docs/RANDOMNESS.md` with:
- Step-by-step verification instructions
- Code examples for each verification check
- Algorithm details for reproducing winner selection
- Randomness source considerations
- Complete audit script example
- Limitations and edge cases

## Verification Algorithm

The key to independent verification is reproducing the winner selection:

```rust
// Rejection sampling (no modulo bias)
let mut rng = initialize_rng(seed);
let mut winners = Vec::new();
let mut used = HashSet::new();

while winners.len() < num_winners {
    let candidate = uniform_u64_in_range(&mut rng, 0, ticket_ids.len());
    if !used.contains(&candidate) {
        winners.push(ticket_ids[candidate]);
        used.insert(candidate);
    }
}
```

This matches `OracleSeedWinnerSelection::select_winner_indices` in `randomness.rs`, ensuring verifiers can independently reproduce the exact same winners.

## Security Considerations

1. **Persistent Storage**: Fairness metadata stored in persistent storage survives ledger-entry expiry
2. **Configuration Hash**: Prevents post-draw config tampering by including hash of effective parameters
3. **State Guards**: Only allows attestation retrieval after finalization prevents premature exposure
4. **Deterministic Hashing**: Uses XDR serialization + SHA-256 for reproducible config hashes

## Testing Recommendations

When cargo/stellar is available, add tests for:

1. **Happy Path**: Finalized raffle returns complete attestation
2. **State Guard**: Pre-finalized raffle returns `InvalidStatus`
3. **Config Hash**: Verify computed hash matches stored parameters
4. **Winner Reproduction**: Use attestation seed to reproduce winners and compare
5. **Storage Persistence**: Verify attestation retrievable after ledger delays
6. **All Randomness Sources**: Test attestation with Internal, External, CommitReveal

## Integration Notes

- **Backwards Compatible**: No changes to existing contract functions
- **Read-Only**: Attestation retrieval never mutates state
- **Gas Efficient**: Single query vs multiple separate queries
- **Storage Cost**: Minimal - only adds metadata hash (32 bytes) to persistent storage

## Future Enhancements

1. **Off-chain Indexer Integration**: Provide reference implementation for indexers to cache attestations
2. **Batch Verification**: Add bulk attestation endpoint for verifying multiple raffles
3. **Commit/Reveal Tracking**: Store commit preimages for CommitReveal verification
4. **VRF Proof Storage**: Store VRF proofs alongside fairness data for External source verification

## Files Changed

1. `contracts/raffle-instance/src/attestation.rs` - New module (147 lines)
2. `contracts/raffle-instance/src/lib.rs` - Added module, DataKey, storage, public function
3. `contracts/raffle-instance/src/views.rs` - Updated documentation
4. `docs/RANDOMNESS.md` - Added 300+ lines of verification documentation

## Code Quality

- Comprehensive inline documentation with rustdoc
- Follows existing codebase patterns and conventions
- Error handling consistent with contract standards
- Security-first design (state guards, persistent storage)

