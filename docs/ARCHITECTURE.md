# Tikka Architecture

This document explains how the factory, raffle instances, oracle, and clients interact.

## Factory -> Instance -> Oracle Flow

```mermaid
graph TB
    UI[Frontend / DApp]
    Factory[RaffleFactory Contract]
    Instance[RaffleInstance Contract]
    Oracle[Oracle Service]
    Stellar[Stellar Network]
    IPFS[IPFS / Metadata]

    UI -->|create_raffle| Factory
    Factory -->|deploys| Instance
    UI -->|buy_tickets| Instance
    UI -->|finalize_raffle| Instance
    Instance -->|RandomnessRequested event| Stellar
    Oracle -->|polls events| Stellar
    Oracle -->|provide_randomness| Instance
    Instance -->|RaffleFinalized event| Stellar
    UI -->|claim_prize| Instance
    UI -->|metadata_hash| IPFS
```

### Flow explanation

1. A creator calls `create_raffle` on the factory with `RaffleConfig`.
2. The factory deploys a new raffle instance and returns the new instance address.
3. Users buy tickets directly on the raffle instance contract.
4. When finalization starts, the instance emits randomness request events to the network.
5. The oracle service polls those events and calls `provide_randomness` back on the instance.
6. The instance finalizes winners, emits finalization events, and winners claim prizes.

## RaffleStatus State Machine

```mermaid
stateDiagram-v2
    [*] --> PendingPrize: create_raffle
    PendingPrize --> Active: deposit_prize
    Active --> Drawing: finalize_raffle / tickets_full
    Active --> Cancelled: cancel_raffle
    Active --> Failed: finalize_raffle (min_tickets not met)
    Drawing --> Finalized: provide_randomness / finalize (internal)
    Drawing --> Cancelled: cancel_raffle / fallback(refund)
    Finalized --> Claimed: all winners claim
    Finalized --> Cancelled: emergency_withdraw
```

### State notes

- `PendingPrize`: created but not funded yet.
- `Active`: funded and selling tickets.
- `Drawing`: draw execution in progress.
- `Finalized`: winners are locked and can claim.
- `Claimed`: terminal state when all claims are complete.
- `Cancelled` / `Failed`: terminal non-success states.

### Entrypoint Lifecycle Transition Matrix

The following table summarizes the behavior of mutating contract entrypoints across all 7 `RaffleStatus` states (#623):

| Mutating Entrypoint | PendingPrize | Active | Drawing | Finalized | Cancelled | Failed | Claimed |
|---|---|---|---|---|---|---|---|
| `deposit_prize` | **Allowed** (-> Active) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) |
| `buy_tickets` | Rejected (`RaffleInactive`) | **Allowed** (-> Active / Drawing) | Rejected (`DrawingAlreadyInProgress` / `RaffleInactive`) | Rejected (`RaffleInactive`) | Rejected (`RaffleInactive`) | Rejected (`RaffleInactive`) | Rejected (`RaffleInactive`) |
| `finalize_raffle` | Rejected (`InvalidStateTransition`) | **Allowed** (if ended/full) | **Allowed** (if Drawing) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) |
| `provide_randomness` | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** (-> Finalized) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) |
| `claim_prize` | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** (-> Finalized / Claimed) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) |
| `cancel_raffle` | **Allowed** (-> Cancelled) | **Allowed** (-> Cancelled) | **Allowed** (-> Cancelled) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** (-> Cancelled) | Rejected (`InvalidStatus`) |
| `refund_ticket` | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** | **Allowed** | Rejected (`InvalidStatus`) |

