# Draw Attestation Usage Guide

## Quick Reference

### For Verifiers

**Goal**: Independently verify a raffle draw was fair without trusting off-chain indexers.

**One-line solution**:
```rust
let attestation = contract.get_draw_attestation(&env)?;
```

### For Integration

**Contract**: `contracts/raffle-instance`

**Function**: `get_draw_attestation() -> Result<DrawAttestation, Error>`

**Availability**: Finalized or Claimed raffles only

**Cost**: Single read-only query (no gas for state changes)

---

## Quick Verification (4 Steps)

### 1. Fetch Attestation
```rust
use tikka_contracts::raffle_instance::DrawAttestation;

let attestation = raffle_client.get_draw_attestation(&env)?;
```

### 2. Verify Config Hash
```rust
// Recompute config hash from attestation fields
let recomputed = hash_config(
    attestation.total_tickets_sold,
    attestation.ticket_price,
    attestation.prize_amount,
    &attestation.prize_distribution_bp,
    &attestation.randomness_source,
);

assert_eq!(recomputed, attestation.config_hash);
```

### 3. Reproduce Winners
```rust
// Use the seed to independently select winners
let reproduced_winners = select_winners_rejection_sampling(
    attestation.fairness_data.seed,
    attestation.total_tickets_sold,
    attestation.prize_distribution_bp.len(),
);

assert_eq!(reproduced_winners, attestation.winning_ticket_ids);
```

### 4. Verify Metadata
```rust
// Fetch off-chain metadata and hash it
let metadata = fetch_from_ipfs(raffle_id);
let computed_hash = sha256(metadata);

assert_eq!(computed_hash, attestation.metadata_hash);
```

---

## Data Structure

### DrawAttestation Fields

| Field | Type | Description |
|---|---|---|
| `fairness_data` | `FairnessData` | Seed, ticket IDs, winning indices, timestamps |
| `metadata_hash` | `BytesN<32>` | SHA-256 of off-chain metadata |
| `winner_addresses` | `Vec<Address>` | Winner addresses in tier order |
| `winning_ticket_ids` | `Vec<u32>` | Winning ticket IDs (1-indexed) |
| `randomness_source` | `RandomnessSource` | Internal, External, or CommitReveal |
| `config_hash` | `BytesN<32>` | Hash of effective raffle config |
| `total_tickets_sold` | `u32` | Total tickets sold |
| `prize_distribution_bp` | `Vec<u32>` | Prize tiers in basis points |
| `prize_amount` | `i128` | Total prize pool |
| `ticket_price` | `i128` | Price per ticket |

### FairnessData (embedded)

| Field | Type | Description |
|---|---|---|
| `seed` | `u64` | Random seed used for winner selection |
| `randomness_source` | `RandomnessSource` | Source of the seed |
| `ticket_ids` | `Vec<u32>` | All ticket IDs in the draw (1..=tickets_sold) |
| `winning_ticket_indices` | `Vec<u32>` | Zero-based winning indices |
| `draw_timestamp` | `u64` | Unix timestamp when draw occurred |
| `draw_sequence` | `u32` | Ledger sequence at draw time |

---

## Winner Selection Algorithm

The contract uses rejection sampling to select winners without modulo bias:

```rust
fn select_winners(seed: u64, ticket_count: u32, winner_count: usize) -> Vec<u32> {
    let mut rng = initialize_prng(seed);
    let mut winners = Vec::new();
    let mut used = HashSet::new();
    
    while winners.len() < winner_count {
        // Generate uniform random in [0, ticket_count)
        let candidate = uniform_u64_in_range(&mut rng, 0, ticket_count as u64);
        
        // Reject if already used
        if !used.contains(&candidate) {
            winners.push(candidate as u32 + 1); // Convert to 1-indexed ticket ID
            used.insert(candidate);
        }
    }
    
    winners
}
```

**Key Points**:
- Uses rejection sampling (no modulo bias)
- Selects distinct winners (no duplicates)
- Ticket IDs are 1-indexed (add 1 to index)
- Matches `OracleSeedWinnerSelection::select_winner_indices`

---

## Error Handling

### InvalidStatus (Error 23)

**When**: Called before raffle is finalized
```rust
let result = contract.try_get_draw_attestation(&env);
match result {
    Err(Ok(Error::InvalidStatus)) => {
        println!("Raffle not finalized yet");
    }
    Ok(attestation) => {
        // Proceed with verification
    }
    _ => panic!("Unexpected error"),
}
```

**Solution**: Only call after raffle status is `Finalized` or `Claimed`

### NotInitialized (Error 43)

**When**: Raffle contract not initialized
**Solution**: Verify contract address is correct

---

## Integration Patterns

### Web3 Frontend

```javascript
// Using Stellar SDK
const contract = new Contract(raffleAddress);

async function verifyDraw(raffleId) {
  // 1. Fetch attestation
  const attestation = await contract.get_draw_attestation();
  
  // 2. Verify config
  const configValid = verifyConfigHash(attestation);
  
  // 3. Reproduce winners
  const winnersValid = reproduceWinners(
    attestation.fairness_data.seed,
    attestation.total_tickets_sold,
    attestation.prize_distribution_bp.length
  );
  
  // 4. Check metadata
  const metadataValid = await verifyMetadata(
    attestation.metadata_hash,
    raffleId
  );
  
  return {
    valid: configValid && winnersValid && metadataValid,
    details: { configValid, winnersValid, metadataValid }
  };
}
```

### Off-chain Indexer

```rust
// Cache attestations for faster lookups
pub struct AttestationCache {
    db: Database,
}

impl AttestationCache {
    pub async fn index_raffle(&self, raffle_id: Address) -> Result<()> {
        let client = RaffleClient::new(&raffle_id);
        
        // Try to fetch attestation
        match client.try_get_draw_attestation() {
            Ok(attestation) => {
                // Store in database for quick access
                self.db.insert_attestation(raffle_id, attestation).await?;
                
                // Verify and store validation status
                let valid = self.verify_attestation(&attestation)?;
                self.db.update_verification_status(raffle_id, valid).await?;
                
                Ok(())
            }
            Err(Error::InvalidStatus) => {
                // Not finalized yet, skip for now
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }
}
```

### Automated Monitoring

```rust
// Continuous verification service
pub struct DrawMonitor {
    client: RaffleClient,
    alert_channel: AlertChannel,
}

impl DrawMonitor {
    pub async fn monitor_raffle(&self, raffle_id: Address) {
        loop {
            sleep(Duration::from_secs(60)).await;
            
            // Check if finalized
            let raffle = self.client.get_raffle(&raffle_id)?;
            if raffle.status != RaffleStatus::Finalized {
                continue;
            }
            
            // Fetch and verify attestation
            let attestation = self.client.get_draw_attestation(&raffle_id)?;
            let audit_result = self.verify_full_attestation(&attestation)?;
            
            if !audit_result.valid {
                // Alert on verification failure
                self.alert_channel.send(Alert {
                    raffle_id,
                    reason: "Draw verification failed",
                    details: audit_result,
                }).await?;
            }
            
            break; // Only monitor each raffle once
        }
    }
}
```

---

## Randomness Source Verification

### External (VRF)

**Extra verification**: Check Ed25519 signature
```rust
if attestation.randomness_source == RandomnessSource::External {
    // Fetch VRF proof from storage or events
    let proof = fetch_vrf_proof(raffle_id)?;
    
    // Build message: (contract, request_id, seed)
    let message = build_vrf_message(
        raffle_id,
        attestation.fairness_data.draw_sequence, // request_id
        attestation.fairness_data.seed,
    );
    
    // Verify Ed25519 signature
    oracle_pubkey.verify(&message, &proof)?;
}
```

### CommitReveal

**Extra verification**: Check commit participation
```rust
if attestation.randomness_source == RandomnessSource::CommitReveal {
    // Count how many tickets committed
    let commit_count = count_commits_from_events(raffle_id)?;
    let participation_rate = commit_count as f64 / attestation.total_tickets_sold as f64;
    
    if participation_rate < 0.5 {
        warn!("Low commit participation ({:.1}%), entropy may be weak", 
              participation_rate * 100.0);
    }
}
```

### Internal/Fallback

**No extra verification needed**, but note:
- Seed is deterministic from ledger state
- Finalizer could have chosen timing
- Only use for low-stakes draws

---

## Common Pitfalls

### 1. Calling Before Finalization
```rust
// ❌ Wrong: Call immediately after buying tickets
client.buy_tickets(&buyer, 5);
let attestation = client.get_draw_attestation(&env)?; // Error: InvalidStatus
```

```rust
// ✅ Correct: Wait for finalization
client.finalize_raffle();
wait_for_finalization();
let attestation = client.get_draw_attestation(&env)?; // OK
```

### 2. Assuming Ticket Storage Exists
```rust
// ❌ Wrong: Ticket records may be wiped
for ticket_id in attestation.winning_ticket_ids {
    let ticket = client.get_ticket(ticket_id); // May fail after wipe_storage
}
```

```rust
// ✅ Correct: Use winner_addresses from attestation
for (i, addr) in attestation.winner_addresses.iter().enumerate() {
    let ticket_id = attestation.winning_ticket_ids[i];
    println!("Winner {}: {} owns ticket {}", i, addr, ticket_id);
}
```

### 3. Off-by-One Errors
```rust
// ❌ Wrong: Indices are 0-based, ticket IDs are 1-based
let winning_index = attestation.fairness_data.winning_ticket_indices[0]; // e.g., 5
let ticket_id = winning_index; // Wrong! Should be 6
```

```rust
// ✅ Correct: Add 1 to convert index to ticket ID
let winning_index = attestation.fairness_data.winning_ticket_indices[0]; // 5
let ticket_id = winning_index + 1; // 6 ✓
```

---

## Performance Notes

- **Query cost**: ~0.01 XLM (read-only, no state changes)
- **Response size**: ~500-2000 bytes (depends on ticket/winner count)
- **Persistent storage**: Survives ledger-entry expiry
- **Gas efficiency**: Single query vs 4+ separate queries

---

## Security Notes

1. **Attestation is read-only**: Cannot modify contract state
2. **State guard**: Only works after finalization (no premature exposure)
3. **Config hash**: Prevents post-draw parameter tampering
4. **Persistent storage**: Fairness data won't expire
5. **Deterministic hashing**: Same config always produces same hash

---

## Further Reading

- **Full verification procedure**: `docs/RANDOMNESS.md` → "Independent Draw Verification"
- **Implementation details**: `DRAW_ATTESTATION_IMPLEMENTATION.md`
- **Randomness modes**: `docs/RANDOMNESS.md`
- **Contract events**: `docs/EVENTS.md`

