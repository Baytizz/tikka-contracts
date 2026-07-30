# Per-Address Ticket Cap Implementation - Complete Changes

## Summary
This implements a configurable per-address ticket cap for raffles, allowing creators to limit how many tickets a single wallet can purchase (e.g., "max 5 tickets per wallet").

## Files Modified

### 1. ✅ contracts/raffle-shared/src/lib.rs
**Status: COMPLETE**
- Added `max_tickets_per_address: u32` field to `RaffleConfig` struct (line ~102)
- Located after `max_tickets_per_tx` field
- Default value of 0 means unlimited (backward compatible)

### 2. contracts/raffle-instance/src/lib.rs  
**Status: NEEDS MANUAL EDIT**

#### Change A: Add field to Raffle struct (around line 59)
```rust
pub max_tickets_per_tx: u32,
pub max_tickets_per_address: u32,  // <-- ADD THIS LINE
pub min_tickets: u32,
```

#### Change B: Add error code to Error enum (around line 186)
```rust
RandomnessTooEarly = 64,
ExceedsMaxTicketsPerAddress = 65,  // <-- ADD THIS LINE
}
```

### 3. contracts/raffle-instance/src/init.rs
**Changes needed:**

#### Change A: Add validation (after line 125, after max_tickets_per_tx validation)
```rust
if config.max_tickets_per_tx == 0 || config.max_tickets_per_tx > config.max_tickets {
    return Err(Error::InvalidParameters);
}
// ADD THESE LINES:
if config.max_tickets_per_address > 0 && config.max_tickets_per_address > config.max_tickets {
    return Err(Error::InvalidParameters);
}
```

#### Change B: Add field to Raffle struct initialization (around line 182-184)
```rust
max_tickets: config.max_tickets,
max_tickets_per_tx: config.max_tickets_per_tx,
max_tickets_per_address: config.max_tickets_per_address,  // <-- ADD THIS LINE
min_tickets: config.min_tickets,
```

### 4. contracts/raffle-instance/src/tickets.rs
**Changes needed in buy_tickets function:**

Add per-address cap check after the existing allow_multiple check (around line 175):

```rust
// Existing code:
if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
    return Err(Error::MultipleTicketsNotAllowed);
}

// ADD THIS NEW CHECK:
// Enforce per-address cap if configured (0 = unlimited)
if raffle.max_tickets_per_address > 0 {
    let new_count = current_count
        .checked_add(quantity)
        .ok_or(Error::ArithmeticOverflow)?;
    if new_count > raffle.max_tickets_per_address {
        return Err(Error::ExceedsMaxTicketsPerAddress);
    }
}
```

### 5. contracts/raffle-factory/src/lib.rs
**Changes needed:**

Add `max_tickets_per_address: 0` to ALL RaffleConfig initializations in tests.
Search for "RaffleConfig {" and add the field to each occurrence.

Example (appears in multiple test functions):
```rust
RaffleConfig {
    description: String::from_str(&env, "Test raffle"),
    end_time: 0,
    no_deadline: true,
    max_tickets: 10,
    max_tickets_per_tx: 10,
    max_tickets_per_address: 0,  // <-- ADD THIS LINE
    min_tickets: 1,
    allow_multiple: true,
    // ... rest of config
}
```

### 6. contracts/raffle-instance/src/test.rs
**Changes needed:**

Same as factory - add `max_tickets_per_address: 0` to ALL RaffleConfig initializations.

Then add the three new test functions from test_per_address_cap.rs file.

### 7. docs/ERRORS.md
**Add to error documentation (around line 180):**

```markdown
| 65   | `ExceedsMaxTicketsPerAddress` | Purchase would exceed per-address ticket cap | "You've reached the maximum tickets allowed per wallet" |
```

### 8. README.md (if exists with raffle config documentation)
Document the new field:
- `max_tickets_per_address`: Maximum tickets one address can own (0 = unlimited)
- Validated to be ≤ max_tickets when non-zero
- Supersedes `allow_multiple` for granular control

## Validation Logic
1. If `max_tickets_per_address == 0`: unlimited (backward compatible, existing behavior)
2. If `max_tickets_per_address > 0`: 
   - Must be `<= max_tickets`
   - Enforced using existing `DataKey::TicketCount(Address)` 
   - Checked before each purchase
   - Error: `ExceedsMaxTicketsPerAddress` (code 65)

## Testing
- Per-address cap enforced across multiple transactions
- Boundary test: exactly at cap succeeds, one over fails
- Zero means unlimited
- Multiple buyers each have independent caps

## Backward Compatibility
✅ Fully backward compatible:
- Default value 0 = unlimited (existing behavior)
- Existing raffles without the field will get 0 automatically
- `allow_multiple` still works but is superseded by the cap

## Next Steps
1. Manually apply changes to lib.rs (2 lines)
2. Apply changes to init.rs (validation + initialization)
3. Apply changes to tickets.rs (enforcement logic)
4. Add field to all test RaffleConfig instances
5. Add new test cases
6. Update error documentation
7. Build and test
