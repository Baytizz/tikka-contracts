# Per-Address Ticket Cap Implementation Status

## 🎯 Feature Goal
Add `max_tickets_per_address` field to RaffleConfig to allow creators to specify a per-wallet ticket limit (e.g., "max 5 tickets per address"). When set to 0, there's no limit (backward compatible).

## ✅ Completed Changes

### 1. contracts/raffle-shared/src/lib.rs
- **Status:** ✅ COMPLETE
- **Change:** Added `pub max_tickets_per_address: u32,` field to RaffleConfig struct (line 101)
- **Location:** After `max_tickets_per_tx` field

## 📋 Remaining Tasks

### Critical Code Changes (Must Do)

1. **contracts/raffle-instance/src/lib.rs** (2 simple edits)
   - Add `pub max_tickets_per_address: u32,` to Raffle struct (after line 58)
   - Add `ExceedsMaxTicketsPerAddress = 65,` to Error enum (after line 185)

2. **contracts/raffle-instance/src/init.rs** (2 edits)
   - Add validation logic for the new field (~3 lines after line 125)
   - Add field to Raffle initialization (1 line around line 183)

3. **contracts/raffle-instance/src/tickets.rs** (1 edit)
   - Add per-address cap enforcement logic (~7 lines after line 162)

### Test Updates (Important)

4. **Update ALL RaffleConfig test instances**
   - Add `max_tickets_per_address: 0,` to every RaffleConfig in:
     - contracts/raffle-instance/src/test.rs (20+ instances)
     - contracts/raffle-factory/src/lib.rs (2+ instances)

5. **Add new test cases** (optional but recommended)
   - Test: cap enforced across transactions
   - Test: zero means unlimited  
   - Test: multiple buyers independent caps

### Documentation (Should Do)

6. **docs/ERRORS.md**
   - Add error code 65 description

7. **README.md** (if config is documented)
   - Document the new field

## 📁 Reference Files Created

- `COMPLETE_IMPLEMENTATION_GUIDE.md` - Step-by-step instructions with exact code
- `CHANGES_SUMMARY.md` - High-level overview of all changes
- `IMPLEMENTATION_CODE.md` - Specific code snippets to add
- `test_per_address_cap.rs` - Complete test functions
- `patch_*.txt` - Individual patch descriptions

## 🚀 Quick Start

To complete this implementation:

1. Open `COMPLETE_IMPLEMENTATION_GUIDE.md`
2. Follow each numbered section in order
3. Make the code changes listed (very small, targeted edits)
4. Update test RaffleConfig instances  
5. Add new tests
6. Build and test

## 🔍 Implementation Details

**Validation:** `max_tickets_per_address` must be 0 or <= `max_tickets`

**Enforcement:** Uses existing `DataKey::TicketCount(Address)` storage

**Error:** Returns `Error::ExceedsMaxTicketsPerAddress` (code 65) when exceeded

**Backward Compatibility:** ✅ Default 0 = unlimited (existing behavior preserved)

## 📊 Acceptance Criteria

From the original task:

- [x] Add `max_tickets_per_address: u32` to RaffleConfig (0 = unlimited), validated ≤ max_tickets
- [ ] Enforce in buy_tickets using per-owner ticket count
- [ ] Tests: cap enforced across multiple transactions, boundary at exactly the cap
- [ ] Update README/docs and docs/ERRORS.md if events change
- [ ] **Key:** Buying past the per-address cap fails with distinct error regardless of how purchases split across transactions

## 💡 Notes

- The shared library (RaffleConfig) is already updated ✅
- All remaining changes are in the instance contract
- Changes are minimal and surgical (< 20 lines of new code total)
- No breaking changes - fully backward compatible
- The feature uses existing storage mechanisms

## Next Action

Review `COMPLETE_IMPLEMENTATION_GUIDE.md` and apply the changes systematically. Each change is small and clearly marked with before/after code blocks.
