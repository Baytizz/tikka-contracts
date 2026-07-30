# Per-Address Ticket Cap - Complete Implementation Guide

## Feature Overview
Adds `max_tickets_per_address` to `RaffleConfig`, allowing creators to set a cap like "max 5 tickets per wallet". When set to 0, there's no limit (backward compatible).

## ✅ COMPLETED
1. **contracts/raffle-shared/src/lib.rs** - Added `max_tickets_per_address: u32` field to RaffleConfig

## 🔄 REMAINING CHANGES

### 1. contracts/raffle-instance/src/lib.rs (2 changes)

#### A. Add field to Raffle struct (line ~59)
Find:
```rust
    pub max_tickets_per_tx: u32,
    pub min_tickets: u32,
```

Change to:
```rust
    pub max_tickets_per_tx: u32,
    pub max_tickets_per_address: u32,
    pub min_tickets: u32,
```

#### B. Add error to Error enum (line ~186)
Find:
```rust
    RandomnessTooEarly = 64,
}
```

Change to:
```rust
    RandomnessTooEarly = 64,
    ExceedsMaxTicketsPerAddress = 65,
}
```

---

### 2. contracts/raffle-instance/src/init.rs (2 changes)

#### A. Add validation (after line ~125)
Find:
```rust
    if config.max_tickets_per_tx == 0 || config.max_tickets_per_tx > config.max_tickets {
        return Err(Error::InvalidParameters);
    }

    if config.ticket_price < MIN_TICKET_PRICE {
```

Change to:
```rust
    if config.max_tickets_per_tx == 0 || config.max_tickets_per_tx > config.max_tickets {
        return Err(Error::InvalidParameters);
    }
    if config.max_tickets_per_address > 0 && config.max_tickets_per_address > config.max_tickets {
        return Err(Error::InvalidParameters);
    }

    if config.ticket_price < MIN_TICKET_PRICE {
```

#### B. Add to Raffle initialization (around line ~182)
Find:
```rust
        max_tickets: config.max_tickets,
        max_tickets_per_tx: config.max_tickets_per_tx,
        min_tickets: config.min_tickets,
```

Change to:
```rust
        max_tickets: config.max_tickets,
        max_tickets_per_tx: config.max_tickets_per_tx,
        max_tickets_per_address: config.max_tickets_per_address,
        min_tickets: config.min_tickets,
```

---

### 3. contracts/raffle-instance/src/tickets.rs (1 change)

#### Add enforcement logic (after line ~162)
Find:
```rust
    if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
        return Err(Error::MultipleTicketsNotAllowed);
    }

    let timestamp = env.ledger().timestamp();
```

Change to:
```rust
    if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
        return Err(Error::MultipleTicketsNotAllowed);
    }

    // Enforce per-address cap if configured (0 = unlimited)
    if raffle.max_tickets_per_address > 0 {
        let new_count = current_count
            .checked_add(quantity)
            .ok_or(Error::ArithmeticOverflow)?;
        if new_count > raffle.max_tickets_per_address {
            return Err(Error::ExceedsMaxTicketsPerAddress);
        }
    }

    let timestamp = env.ledger().timestamp();
```

---

### 4. Update ALL RaffleConfig instances in tests

#### Files to update:
- contracts/raffle-instance/src/test.rs (multiple instances)
- contracts/raffle-factory/src/lib.rs (multiple instances)

#### Change pattern:
Find every occurrence of:
```rust
RaffleConfig {
    // ... fields ...
    max_tickets_per_tx: VALUE,
    min_tickets: VALUE,
```

Add after max_tickets_per_tx:
```rust
    max_tickets_per_tx: VALUE,
    max_tickets_per_address: 0,  // 0 = unlimited for existing tests
    min_tickets: VALUE,
```

**Quick find/replace approach:**
Search for: `max_tickets_per_tx: `
In each RaffleConfig block, manually add the new line after it.

---

### 5. docs/ERRORS.md

Add to the error table (after error 64):
```markdown
| 65   | `ExceedsMaxTicketsPerAddress` | Purchase would exceed per-address ticket cap | "You've reached the maximum tickets allowed per wallet" |
```

---

## New Test Cases

Add to contracts/raffle-instance/src/test.rs:

```rust
#[test]
fn test_per_address_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Per-address cap test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        max_tickets_per_address: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1_000_000,
        payment_token: token.address.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    token.mint(&buyer, &20_000_000);
    token.mint(&creator, &10_000_000);
    client.deposit_prize();

    // Buy 3 tickets
    client.buy_tickets(&buyer, &3);
    
    // Buy 2 more (total = 5, at cap)
    client.buy_tickets(&buyer, &2);

    // Try to buy 1 more - should fail
    let result = client.try_buy_tickets(&buyer, &1);
    assert_eq!(result, Err(Ok(Error::ExceedsMaxTicketsPerAddress)));
}

#[test]
fn test_per_address_cap_zero_means_unlimited() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Unlimited"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        max_tickets_per_address: 0, // Unlimited
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1_000_000,
        payment_token: token.address.clone(),
        prize_amount: 10_000_000,
        prizes: Vec::from_array(&env, [10000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    token.mint(&buyer, &20_000_000);
    token.mint(&creator, &10_000_000);
    client.deposit_prize();

    // Buy all 10 tickets
    client.buy_tickets(&buyer, &5);
    let result = client.buy_tickets(&buyer, &5);
    assert!(result == 10);
}
```

---

## Verification Checklist

After making all changes, verify:

- [ ] `cargo build` succeeds for raffle-instance
- [ ] `cargo build` succeeds for raffle-factory
- [ ] `cargo test` passes all existing tests
- [ ] New per-address cap tests pass
- [ ] Error code 65 documented in ERRORS.md
- [ ] All RaffleConfig instances have the new field

## Summary

**Total changes:** 
- 2 lines in lib.rs
- 2 blocks in init.rs  
- 1 block in tickets.rs
- ~40+ test config updates
- 2 new test functions
- 1 error documentation entry

**Backward compatibility:** ✅ Fully compatible (0 = unlimited)

**Breaking changes:** ❌ None (new optional field with safe default)
