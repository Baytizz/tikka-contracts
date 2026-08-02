# Per-Address Ticket Cap Implementation Plan

## Changes Required

### 1. contracts/raffle-shared/src/lib.rs
- ✅ Added `max_tickets_per_address: u32` field to `RaffleConfig` struct after `max_tickets_per_tx`

### 2. contracts/raffle-instance/src/lib.rs
- Add `max_tickets_per_address: u32` field to `Raffle` struct after `max_tickets_per_tx`
- Add `ExceedsMaxTicketsPerAddress = 65` to `Error` enum after `RandomnessTooEarly`

### 3. contracts/raffle-instance/src/init.rs
- Add validation: `max_tickets_per_address` must be `0` or `<= max_tickets`
- Pass `max_tickets_per_address` from config to Raffle struct initialization

### 4. contracts/raffle-instance/src/tickets.rs (buy_tickets function)
- Check per-address cap before allowing purchase:
  ```rust
  if raffle.max_tickets_per_address > 0 {
      let new_count = current_count + quantity;
      if new_count > raffle.max_tickets_per_address {
          return Err(Error::ExceedsMaxTicketsPerAddress);
      }
  }
  ```

### 5. Tests (contracts/raffle-instance/src/test.rs)
- Add test for per-address cap enforcement
- Test boundary conditions (exactly at cap)
- Test cap across multiple transactions

### 6. Documentation
- Update README.md
- Update docs/ERRORS.md with new error code
- Update docs/EVENTS.md if needed

## Validation Logic
- If `max_tickets_per_address == 0`, unlimited tickets per address (backward compatible)
- If `max_tickets_per_address > 0`, must be `<= max_tickets`
- Enforced using existing `DataKey::TicketCount(Address)` storage

## Backwards Compatibility
- Default value of 0 means no cap (unlimited), maintaining existing behavior
- `allow_multiple` boolean still works but is superseded by the cap when set
