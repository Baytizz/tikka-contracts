# Error Codes Documentation

This document is generated from the contract error enums. Regenerate with:

```bash
python3 scripts/generate_error_docs.py
```

Sources:
- `Error` — `raffle-instance/src/lib.rs`
- `ContractError` — `raffle-factory/src/lib.rs`

## Table of Contents

- [Instance Contract Errors](#instance-contract-errors)
- [Factory Contract Errors](#factory-contract-errors)

---

## Instance Contract Errors

The instance contract (`RaffleInstance`) handles individual raffle operations.

Source enum: `Error` in [`raffle-instance/src/lib.rs`](raffle-instance/src/lib.rs)

| Code | Error | Description | Contract | Frontend Message |
| ---- | ----- | ----------- | -------- | ---------------- |
| 1 | `RaffleNotFound` | Raffle storage entry is missing. Code 1. | RaffleInstance | "Raffle storage entry is missing" |
| 2 | `RaffleInactive` | Raffle is not in an active state for ticket sales. Code 2. | RaffleInstance | "Raffle is not in an active state for ticket sales" |
| 3 | `TicketsSoldOut` | All tickets have been sold. Code 3. | RaffleInstance | "All tickets have been sold" |
| 4 | `InsufficientFunds` | Caller balance is insufficient for the operation. Code 4. | RaffleInstance | "Caller balance is insufficient for the operation" |
| 5 | `NotAuthorized` | Caller is not authorized for this action. Code 5. | RaffleInstance | "Caller is not authorized for this action" |
| 6 | `OracleNotSet` | External randomness requested but oracle is not configured. Code 6. | RaffleInstance | "External randomness requested but oracle is not configured" |
| 7 | `RandomnessAlreadyRequested` | Randomness was already requested for this draw. Code 7. | RaffleInstance | "Randomness was already requested for this draw" |
| 8 | `NoRandomnessRequest` | No pending randomness request exists. Code 8. | RaffleInstance | "No pending randomness request exists" |
| 9 | `FallbackTooEarly` | Fallback randomness cannot be used yet. Code 9. | RaffleInstance | "Fallback randomness cannot be used yet" |
| 11 | `PrizeNotDeposited` | Prize has not been deposited by the creator. Code 11. | RaffleInstance | "Prize has not been deposited by the creator" |
| 12 | `PrizeAlreadyClaimed` | Prize tier was already claimed or swept. Code 12. | RaffleInstance | "Prize tier was already claimed or swept" |
| 13 | `PrizeAlreadyDeposited` | Prize deposit was already completed. Code 13. | RaffleInstance | "Prize deposit was already completed" |
| 14 | `NotWinner` | Caller is not the winner for this tier. Code 14. | RaffleInstance | "Caller is not the winner for this tier" |
| 15 | `ClaimTooEarly` | Claim or sweep attempted before the configured delay elapsed. Code 15. | RaffleInstance | "Claim or sweep attempted before the configured delay elapsed" |
| 21 | `InvalidParameters` | One or more input parameters are invalid. Code 21. | RaffleInstance | "One or more input parameters are invalid" |
| 22 | `InvalidQuantity` | Ticket quantity is out of range. Code 22. | RaffleInstance | "Ticket quantity is out of range" |
| 23 | `InvalidStatus` | Raffle status does not allow this operation. Code 23. | RaffleInstance | "Raffle status does not allow this operation" |
| 24 | `ContractPaused` | Contract is paused. Code 24. | RaffleInstance | "Contract is paused" |
| 25 | `InvalidStateTransition` | Requested lifecycle transition is not allowed. Code 25. | RaffleInstance | "Requested lifecycle transition is not allowed" |
| 26 | `RaffleExpired` | Raffle end time has passed. Code 26. | RaffleInstance | "Raffle end time has passed" |
| 31 | `InsufficientTickets` | Minimum ticket threshold was not met. Code 31. | RaffleInstance | "Minimum ticket threshold was not met" |
| 32 | `MultipleTicketsNotAllowed` | Address already holds a ticket when multiples are disallowed. Code 32. | RaffleInstance | "Address already holds a ticket when multiples are disallowed" |
| 33 | `NoTicketsSold` | No tickets were sold. Code 33. | RaffleInstance | "No tickets were sold" |
| 34 | `TicketNotFound` | Ticket record was not found. Code 34. | RaffleInstance | "Ticket record was not found" |
| 35 | `RaffleEnded` | Raffle has already ended. Code 35. | RaffleInstance | "Raffle has already ended" |
| 41 | `ArithmeticOverflow` | Integer overflow in a contract calculation. Code 41. | RaffleInstance | "Integer overflow in a contract calculation" |
| 42 | `AlreadyInitialized` | Contract initialization was already performed. Code 42. | RaffleInstance | "Contract initialization was already performed" |
| 43 | `NotInitialized` | Contract has not been initialized. Code 43. | RaffleInstance | "Contract has not been initialized" |
| 44 | `Reentrancy` | Reentrant call detected. Code 44. | RaffleInstance | "Reentrant call detected" |
| 45 | `TokenTransferFailed` | Token transfer failed. Code 45. | RaffleInstance | "Token transfer failed" |
| 46 | `NoActiveTickets` | No active tickets remain for the operation. Code 46. | RaffleInstance | "No active tickets remain for the operation" |
| 47 | `DeadlinePassed` | Token swap deadline has passed. Code 47. | RaffleInstance | "Token swap deadline has passed" |
| 48 | `SlippageExceeded` | Swap output below slippage tolerance. Code 48. | RaffleInstance | "Swap output below slippage tolerance" |
| 49 | `InvalidIndex` | Index is out of bounds. Code 49. | RaffleInstance | "Index is out of bounds" |
| 50 | `MorePrizesThanTickets` | More prize tiers configured than tickets sold. Code 50. | RaffleInstance | "More prize tiers configured than tickets sold" |
| 51 | `ZeroPrize` | Computed prize amount is zero. Code 51. | RaffleInstance | "Computed prize amount is zero" |
| 52 | `InvalidTokenAddress` | Token address is invalid or unsupported. Code 52. | RaffleInstance | "Token address is invalid or unsupported" |
| 53 | `TooManyPrizes` | Prize tier count exceeds protocol maximum. Code 53. | RaffleInstance | "Prize tier count exceeds protocol maximum" |
| 54 | `EmergencyTooEarly` | Emergency withdraw attempted before the delay elapsed. Code 54. | RaffleInstance | "Emergency withdraw attempted before the delay elapsed" |
| 55 | `InvalidTicketRange` | Minimum tickets exceed maximum tickets. Code 55. | RaffleInstance | "Minimum tickets exceed maximum tickets" |
| 56 | `InsufficientAccumulatedFees` | Accumulated fees are below the requested withdrawal. Code 56. | RaffleInstance | "Accumulated fees are below the requested withdrawal" |
| 57 | `PrizeConfigurationLocked` | Prize configuration is locked after deposits or sales. Code 57. | RaffleInstance | "Prize configuration is locked after deposits or sales" |
| 58 | `ExceedsMaxTicketsPerTx` | Ticket purchase exceeds per-transaction cap. Code 58. | RaffleInstance | "Ticket purchase exceeds per-transaction cap" |
| 59 | `DrawingAlreadyInProgress` | Draw already in progress. Code 59. | RaffleInstance | "Draw already in progress" |
| 60 | `InvalidStatusForDrawingTransition` | Invalid status for entering the drawing phase. Code 60. | RaffleInstance | "Invalid status for entering the drawing phase" |
| 61 | `DrawingAlreadyComplete` | Draw has already completed. Code 61. | RaffleInstance | "Draw has already completed" |
| 62 | `InvalidEndTime` | End time is in the past or otherwise invalid. Code 62. | RaffleInstance | "End time is in the past or otherwise invalid" |
| 63 | `InvalidAdminAddress` | Admin address is zero, self, or otherwise invalid. Code 63. | RaffleInstance | "Admin address is zero, self, or otherwise invalid" |
| 64 | `RandomnessTooEarly` | Randomness callback received before the minimum delay. Code 64. | RaffleInstance | "Randomness callback received before the minimum delay" |
| 65 | `ExceedsMaxTicketsPerAddress` | Per-address ticket cap would be exceeded. Code 65. | RaffleInstance | "Per-address ticket cap would be exceeded" |
| 67 | `CancelTimelockActive` | Admin cancellation timelock has not elapsed. Code 67. | RaffleInstance | "Admin cancellation timelock has not elapsed" |
| 68 | `CancelNotScheduled` | Admin cancellation was not scheduled. Code 68. | RaffleInstance | "Admin cancellation was not scheduled" |
| 69 | `OracleNotRegistered` | Oracle address is not in the quorum allowlist. Code 69. | RaffleInstance | "Oracle address is not in the quorum allowlist" |
| 70 | `DuplicateOracleSubmission` | Oracle already submitted a seed for this draw. Code 70. | RaffleInstance | "Oracle already submitted a seed for this draw" |

---

## Factory Contract Errors

The factory contract (`RaffleFactory`) manages raffle creation.

Source enum: `ContractError` in [`raffle-factory/src/lib.rs`](raffle-factory/src/lib.rs)

| Code | Error | Description | Contract | Frontend Message |
| ---- | ----- | ----------- | -------- | ---------------- |
| 1 | `AlreadyInitialized` | `init_factory` was called on an already-initialized contract. Code 1. | RaffleFactory | "`init_factory` was called on an already-initialized contract" |
| 2 | `NotAuthorized` | Caller is not the admin or the operation requires admin authorization. Code 2. | RaffleFactory | "Caller is not the admin or the operation requires admin authorization" |
| 3 | `ContractPaused` | The factory is paused; raffle creation is blocked until unpaused. Code 3. | RaffleFactory | "The factory is paused; raffle creation is blocked until unpaused" |
| 4 | `InvalidParameters` | A supplied parameter is out of range or otherwise invalid (e.g., fee exceeds [`MAX_PROTOCOL_FEE_BP`], zero/self address). Code 4. | RaffleFactory | "A supplied parameter is out of range or otherwise invalid (e" |
| 5 | `RaffleNotFound` | The requested raffle stable-ID does not map to an existing contract. Code 5. | RaffleFactory | "The requested raffle stable-ID does not map to an existing contract" |
| 11 | `AdminTransferPending` | A two-step admin transfer is already in progress; the current proposal must be accepted or cancelled before a new one can be opened. Code 11. | RaffleFactory | "A two-step admin transfer is already in progress; the current proposal must be accepted or cancelled before a new one can be opened" |
| 12 | `NoPendingTransfer` | `accept_factory_admin` was called but there is no pending transfer. Code 12. | RaffleFactory | "`accept_factory_admin` was called but there is no pending transfer" |
| 13 | `RateLimitExceeded` | A non-whitelisted creator attempted to create a raffle before the [`MinCreationDelay`](DataKey::MinCreationDelay) window elapsed. Code 13. | RaffleFactory | "A non-whitelisted creator attempted to create a raffle before the [`MinCreationDelay`](DataKey::MinCreationDelay) window elapsed" |
| 14 | `NoPendingOp` | `execute_config_change` or `cancel_config_change` was called with an `op_id` that has no pending operation. Code 14. | RaffleFactory | "`execute_config_change` or `cancel_config_change` was called with an `op_id` that has no pending operation" |
| 15 | `TimelockNotElapsed` | `execute_config_change` was called before `effective_timestamp` was reached. Code 15. | RaffleFactory | "`execute_config_change` was called before `effective_timestamp` was reached" |
| 16 | `InvalidRaffleId` | `clean_old_raffle` was called with an ID that is not in the stable-map (never assigned or already tombstoned). Code 16. | RaffleFactory | "`clean_old_raffle` was called with an ID that is not in the stable-map (never assigned or already tombstoned)" |
| 17 | `RaffleNotEligible` | Reserved for future use — a raffle does not meet eligibility criteria for the requested operation. Code 17. | RaffleFactory | "Reserved for future use — a raffle does not meet eligibility criteria for the requested operation" |
| 18 | `ArithmeticOverflow` | A `checked_add` overflow occurred while accumulating volume. Code 18. | RaffleFactory | "A `checked_add` overflow occurred while accumulating volume" |
| 19 | `TreasuryNotSet` | `create_raffle` could not read the treasury address (factory not fully initialized). Code 19. | RaffleFactory | "`create_raffle` could not read the treasury address (factory not fully initialized)" |
| 20 | `RecurringNotFound` | Recurring raffle schedule was not found. Code 20. | RaffleFactory | "Recurring raffle schedule was not found" |
| 21 | `IntervalNotElapsed` | Recurring round interval has not elapsed yet. Code 21. | RaffleFactory | "Recurring round interval has not elapsed yet" |
| 22 | `MaxRoundsReached` | Recurring raffle reached its configured maximum rounds. Code 22. | RaffleFactory | "Recurring raffle reached its configured maximum rounds" |
| 23 | `RecurringInactive` | Recurring raffle schedule is inactive. Code 23. | RaffleFactory | "Recurring raffle schedule is inactive" |
| 24 | `CreationPaused` | `create_raffle` was called while creation is paused via `set_creation_paused` (#611). Distinct from `ContractPaused`, which blocks the whole factory. Code 24. | RaffleFactory | "`create_raffle` was called while creation is paused via `set_creation_paused` (#611)" |
