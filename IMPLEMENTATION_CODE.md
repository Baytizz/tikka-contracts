# Exact Code to Add

## File: contracts/raffle-instance/src/tickets.rs

### Location: After line 162 (after the allow_multiple check)

Insert this code block:

```rust
    if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
        return Err(Error::MultipleTicketsNotAllowed);
    }

    // ADD THE FOLLOWING LINES HERE:
    // Enforce per-address cap if configured (0 = unlimited)
    if raffle.max_tickets_per_address > 0 {
        let new_count = current_count
            .checked_add(quantity)
            .ok_or(Error::ArithmeticOverflow)?;
        if new_count > raffle.max_tickets_per_address {
            return Err(Error::ExceedsMaxTicketsPerAddress);
        }
    }
    // END OF NEW CODE

    let timestamp = env.ledger().timestamp();
```

This adds the per-address ticket cap enforcement right after the existing allow_multiple check and before price calculation.
