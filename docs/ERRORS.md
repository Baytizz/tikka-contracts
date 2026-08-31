# Error Codes Documentation

This document describes all error codes used in the Tikka Raffle protocol.
Frontend applications can use these codes to display user-friendly error messages.

> **Note:** To keep this documentation in sync with the Rust error enums, run the generation script:
>
> ```bash
> python scripts/generate_error_docs.py
> ```
>
> This script parses `contracts/raffle-instance/src/lib.rs`, `contracts/raffle-factory/src/lib.rs`, and `contracts/raffle-shared/src/errors.rs` and outputs the current error codes and their mappings.

## Table of Contents

- [Instance Contract Errors](#instance-contract-errors)
- [Factory Contract Errors](#factory-contract-errors)
- [Error Code Mapping](#error-code-mapping)

---

## Instance Contract Errors

The instance contract (`Raffle`) handles individual raffle operations. All error codes are defined in the `Error` enum in [`contracts/raffle-instance/src/lib.rs`](contracts/raffle-instance/src/lib.rs).

### Protocol Fee Model

The protocol fee (`protocol_fee_bp`) is charged **once**, at ticket purchase. Winners receive the full gross prize amount on claim; `PrizeClaimed.platform_fee` is always `0`. Total protocol revenue equals the sum of fees collected from ticket sales.

### General Errors (1-15)

| Code | Error                        | Description                                   | Frontend Message                                |
| ---- | ---------------------------- | --------------------------------------------- | ----------------------------------------------- |
| 1    | `RaffleNotFound`             | The raffle data was not found in storage      | "Raffle not found"                              |
| 2    | `RaffleInactive`             | The raffle is not in an active state          | "This raffle is not currently active"           |
| 3    | `TicketsSoldOut`             | All tickets have been sold                    | "Sorry, all tickets have been sold!"            |
| 4    | `InsufficientFunds`          | User does not have enough balance             | "Insufficient funds to complete this action"    |
| 5    | `NotAuthorized`              | User is not authorized to perform this action | "You are not authorized to perform this action" |
| 6    | `OracleNotSet`               | Oracle address is not configured              | "Oracle address is not set"                     |
| 7    | `RandomnessAlreadyRequested` | Randomness has already been requested          | "Randomness request already in progress"        |
| 8    | `NoRandomnessRequest`        | No randomness request found                   | "No randomness request found"                  |
| 9    | `FallbackTooEarly`           | Fallback randomness triggered too early       | "Fallback randomness not available yet"         |
| 11   | `PrizeNotDeposited`          | Prize has not been deposited yet              | "Prize not yet deposited"                       |
| 12   | `PrizeAlreadyClaimed`        | Prize has already been claimed                | "Prize has already been claimed"                |
| 13   | `PrizeAlreadyDeposited`      | Prize deposit was already completed           | "Prize has already been deposited"              |
| 14   | `NotWinner`                  | Only the winner can claim the prize           | "You are not the winner of this raffle"         |
| 15   | `ClaimTooEarly`              | Cannot claim before cooldown period           | "Please wait before claiming your prize"        |

### State/Validation Errors (21-26)

| Code | Error                    | Description                                            | Frontend Message                                         |
| ---- | ------------------------ | ------------------------------------------------------ | -------------------------------------------------------- |
| 21   | `InvalidParameters`      | Invalid input parameters provided                      | "Invalid parameters provided"                            |
| 22   | `InvalidQuantity`        | Invalid ticket quantity requested                      | "Invalid ticket quantity"                                |
| 23   | `InvalidStatus`          | The current raffle status doesn't allow this operation | "This action is not allowed in the current raffle state" |
| 24   | `ContractPaused`         | The contract is paused                                 | "Contract is temporarily paused"                         |
| 25   | `InvalidStateTransition` | Cannot transition to the requested state               | "Cannot change raffle to the requested state"            |
| 26   | `RaffleExpired`          | The raffle end time has passed                         | "This raffle has ended"                                  |

### Ticket Errors (31-35)

| Code | Error                       | Description                         | Frontend Message                               |
| ---- | --------------------------- | ----------------------------------- | ---------------------------------------------- |
| 31   | `InsufficientTickets`       | Not enough tickets sold to finalize | "Minimum ticket requirement not met"           |
| 32   | `MultipleTicketsNotAllowed` | User already has a ticket           | "Multiple tickets not allowed for this raffle" |
| 33   | `NoTicketsSold`             | No tickets have been purchased      | "No tickets have been sold yet"                |
| 34   | `TicketNotFound`            | The requested ticket was not found  | "Ticket not found"                             |
| 35   | `RaffleEnded`               | The raffle has already ended         | "This raffle has already ended"                |

### System Errors (41-64)

| Code | Error                    | Description                       | Frontend Message               |
| ---- | ------------------------ | --------------------------------- | ------------------------------ |
| 41   | `ArithmeticOverflow`     | Arithmetic operation overflow     | "Calculation error occurred"   |
| 42   | `AlreadyInitialized`     | Contract is already initialized   | "Contract already initialized" |
| 43   | `NotInitialized`         | Contract has not been initialized | "Contract not initialized"     |
| 44   | `Reentrancy`             | Reentrant call detected           | "Please try again later"       |
| 45   | `TokenTransferFailed`    | Token transfer failed             | "Token transfer failed"        |
| 46   | `NoActiveTickets`        | No active tickets available       | "No active tickets available"  |
| 47   | `DeadlinePassed`         | Swap deadline has passed          | "Swap deadline has passed"     |
| 48   | `SlippageExceeded`       | Slippage tolerance exceeded       | "Slippage tolerance exceeded"  |
| 49   | `InvalidIndex`           | Invalid index provided            | "Invalid index provided"       |
| 50   | `MorePrizesThanTickets`  | More prizes than tickets          | "More prizes than tickets"     |
| 51   | `ZeroPrize`              | Prize amount is zero              | "Prize amount cannot be zero"  |
| 52   | `InvalidTokenAddress`    | Invalid token address provided    | "Invalid token address"        |
| 53   | `TooManyPrizes`          | Exceeds maximum number of prizes  | "Too many prizes configured"   |
| 54   | `EmergencyTooEarly`      | Emergency withdraw too early      | "Emergency withdraw not available yet" |
| 55   | `InvalidTicketRange`     | Invalid ticket range configured   | "Invalid ticket range"         |
| 56   | `InsufficientAccumulatedFees` | Insufficient accumulated fees | "Insufficient accumulated fees" |
| 57   | `PrizeConfigurationLocked` | Prize configuration is locked   | "Prize configuration is locked" |
| 58   | `ExceedsMaxTicketsPerTx` | Exceeds max tickets per transaction | "Too many tickets for one transaction" |
| 59   | `DrawingAlreadyInProgress` | A draw is already in progress   | "Drawing already in progress"  |
| 60   | `DrawingAlreadyComplete` | Randomness was already provided   | "Drawing already complete"     |
| 61   | `InvalidEndTime`         | Raffle end time is invalid        | "Invalid raffle end time"      |
| 62   | `InvalidAdminAddress`    | Admin address is invalid          | "Invalid admin address"        |
| 63   | `InvalidStatusForDrawingTransition` | Raffle status cannot enter Drawing | "Cannot start drawing in current state" |
| 64   | `RandomnessTooEarly`     | Randomness request too early      | "Randomness request too early" |

---

## Factory Contract Errors

The factory contract (`RaffleFactory`) manages raffle creation. All error codes are defined in the `ContractError` enum in [`contracts/raffle-factory/src/lib.rs`](contracts/raffle-factory/src/lib.rs).

### General Errors (1-5)

| Code | Error                | Description                    | Frontend Message                |
| ---- | -------------------- | ------------------------------ | ------------------------------- |
| 1    | `AlreadyInitialized` | Factory is already initialized | "Factory already initialized"   |
| 2    | `NotAuthorized`      | User is not the admin          | "You are not the admin"         |
| 3    | `ContractPaused`     | Factory is paused              | "Factory is temporarily paused" |
| 4    | `InvalidParameters`  | Invalid parameters provided    | "Invalid parameters provided"   |
| 5    | `RaffleNotFound`     | Raffle instance not found      | "Raffle not found"              |

### Admin & Operation Errors (11-19)

| Code | Error                  | Description                    | Frontend Message                 |
| ---- | ---------------------- | ------------------------------ | -------------------------------- |
| 11   | `AdminTransferPending` | Admin transfer already pending | "Admin transfer already pending" |
| 12   | `NoPendingTransfer`    | No pending admin transfer      | "No pending admin transfer"      |
| 13   | `RateLimitExceeded`    | Raffle creation rate limited   | "Raffle creation rate limit exceeded" |
| 14   | `NoPendingOp`          | No pending operation           | "No pending operation"           |
| 15   | `TimelockNotElapsed`   | Timelock delay not elapsed     | "Timelock delay has not elapsed" |
| 16   | `InvalidRaffleId`      | Invalid raffle stable-ID       | "Invalid raffle ID"              |
| 17   | `RaffleNotEligible`    | Raffle not eligible            | "Raffle is not eligible for this operation" |
| 18   | `ArithmeticOverflow`   | Arithmetic overflow            | "Calculation error occurred"     |
| 19   | `TreasuryNotSet`       | Treasury address not set       | "Treasury address is not set"    |

---

## Error Code Mapping

### JavaScript/TypeScript Example

```typescript
// Frontend error mapping
const errorMessages: Record<number, string> = {
  // Instance errors (1-64)
  1: "Raffle not found",
  2: "This raffle is not currently active",
  3: "Sorry, all tickets have been sold!",
  4: "Insufficient funds to complete this action",
  5: "You are not authorized to perform this action",
  6: "Oracle address is not set",
  7: "Randomness request already in progress",
  8: "No randomness request found",
  9: "Fallback randomness not available yet",
  11: "Prize not yet deposited",
  12: "Prize has already been claimed",
  13: "Prize has already been deposited",
  14: "You are not the winner of this raffle",
  15: "Please wait before claiming your prize",
  21: "Invalid parameters provided",
  22: "Invalid ticket quantity",
  23: "This action is not allowed in the current raffle state",
  24: "Contract is temporarily paused",
  25: "Cannot change raffle to the requested state",
  26: "This raffle has ended",
  31: "Minimum ticket requirement not met",
  32: "Multiple tickets not allowed for this raffle",
  33: "No tickets have been sold yet",
  34: "Ticket not found",
  35: "This raffle has already ended",
  41: "Calculation error occurred",
  42: "Contract already initialized",
  43: "Contract not initialized",
  44: "Please try again later",
  45: "Token transfer failed",
  46: "No active tickets available",
  47: "Swap deadline has passed",
  48: "Slippage tolerance exceeded",
  49: "Invalid index provided",
  50: "More prizes than tickets",
  51: "Prize amount cannot be zero",
  52: "Invalid token address",
  53: "Too many prizes configured",
  54: "Emergency withdraw not available yet",
  55: "Invalid ticket range",
  56: "Insufficient accumulated fees",
  57: "Prize configuration is locked",
  58: "Too many tickets for one transaction",
  59: "Drawing already in progress",
  60: "Drawing already complete",
  61: "Invalid raffle end time",
  62: "Invalid admin address",
  63: "Cannot start drawing in current state",
  64: "Randomness request too early",

  // Factory errors (1-19)
  1: "Factory already initialized",
  2: "You are not the admin",
  3: "Factory is temporarily paused",
  4: "Invalid parameters provided",
  5: "Raffle not found",
  11: "Admin transfer already pending",
  12: "No pending admin transfer",
  13: "Raffle creation rate limit exceeded",
  14: "No pending operation",
  15: "Timelock delay has not elapsed",
  16: "Invalid raffle ID",
  17: "Raffle is not eligible for this operation",
  18: "Calculation error occurred",
  19: "Treasury address is not set",
};

function handleContractError(errorCode: number, contract: 'instance' | 'factory'): string {
  return errorMessages[errorCode] || "An unknown error occurred";
}
```

### React Example

```tsx
import React from "react";

interface ErrorDisplayProps {
  errorCode: number;
  contract: 'instance' | 'factory';
}

const ERROR_MESSAGES: Record<number, string> = {
  // Instance errors
  1: "Raffle not found",
  2: "This raffle is not currently active",
  3: "Sorry, all tickets have been sold!",
  4: "Insufficient funds. Please top up your wallet.",
  5: "You are not authorized to perform this action",
  6: "Oracle address is not set",
  7: "Randomness request already in progress",
  8: "No randomness request found",
  9: "Fallback randomness not available yet",
  11: "Prize not yet deposited",
  12: "Prize has already been claimed",
  13: "Prize has already been deposited",
  14: "You are not the winner of this raffle",
  15: "Please wait before claiming your prize",
  21: "Invalid parameters provided",
  22: "Invalid ticket quantity",
  23: "This action is not allowed in the current raffle state",
  24: "Contract is temporarily paused",
  25: "Cannot change raffle to the requested state",
  26: "This raffle has ended",
  31: "Minimum ticket requirement not met",
  32: "Multiple tickets not allowed for this raffle",
  33: "No tickets have been sold yet",
  34: "Ticket not found",
  35: "This raffle has already ended",
  41: "Calculation error occurred",
  42: "Contract already initialized",
  43: "Contract not initialized",
  44: "Please try again later",
  45: "Token transfer failed",
  46: "No active tickets available",
  47: "Swap deadline has passed",
  48: "Slippage tolerance exceeded",
  49: "Invalid index provided",
  50: "More prizes than tickets",
  51: "Prize amount cannot be zero",
  52: "Invalid token address",
  53: "Too many prizes configured",
  54: "Emergency withdraw not available yet",
  55: "Invalid ticket range",
  56: "Insufficient accumulated fees",
  57: "Prize configuration is locked",
  58: "Too many tickets for one transaction",
  59: "Drawing already in progress",
  60: "Drawing already complete",
  61: "Invalid raffle end time",
  62: "Invalid admin address",
  63: "Cannot start drawing in current state",
  64: "Randomness request too early",
  // Factory errors
  1: "Factory already initialized",
  2: "You are not the admin",
  3: "Factory is temporarily paused",
  4: "Invalid parameters provided",
  5: "Raffle not found",
  11: "Admin transfer already pending",
  12: "No pending admin transfer",
  13: "Raffle creation rate limit exceeded",
  14: "No pending operation",
  15: "Timelock delay has not elapsed",
  16: "Invalid raffle ID",
  17: "Raffle is not eligible for this operation",
  18: "Calculation error occurred",
  19: "Treasury address is not set",
};

export const ErrorDisplay: React.FC<ErrorDisplayProps> = ({ errorCode }) => {
  const message =
    ERROR_MESSAGES[errorCode] || "An error occurred. Please try again.";

  return (
    <div className="error-message">
      <span className="error-icon">⚠️</span>
      <span>{message}</span>
    </div>
  );
};
```

---

## Testing Error Handling

All error codes should be tested in the contract test suite to ensure proper error propagation. Run tests with:

```bash
cargo test -p raffle-factory
cargo test -p raffle-instance
```

---

## Best Practices

1. **Always use Result types**: Never use `panic!()` or `expect()` in production code
1. **Provide meaningful error codes**: Use descriptive error codes that frontend can map to user messages
1. **Document all errors**: Keep this file updated with any new error codes
1. **Handle edge cases**: Test all error paths to ensure proper error propagation
1. **Use appropriate error granularity**: Different errors should have different codes for better UX

---

## Error Code Ranges

To prevent collisions as the protocol evolves, error codes are reserved in non-overlapping ranges:

| Range     | Owner            | Purpose                                      |
| --------- | ---------------- | -------------------------------------------- |
| 1 – 99    | Shared           | Conditions used by both instance and factory  |
| 100 – 199 | Instance-only    | New instance-specific errors                 |
| 200 – 299 | Factory-only     | New factory-specific errors                  |

**Rule:** codes are append-only within a range and are never reused. See `CONTRIBUTING.md` for the full policy.
