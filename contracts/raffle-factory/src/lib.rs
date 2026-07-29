#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, xdr::ToXdr, Address, Bytes, BytesN,
    Env, IntoVal, Symbol, Vec,
};

#[cfg(test)]
use soroban_sdk::testutils::Address as _;

mod events;

use raffle_shared::{
    effective_limit, AdminOp, FairnessData, PageResultRaffles, PaginationParams, RaffleConfig,
};

use raffle_shared::constants::{CHECKPOINT_INTERVAL, MAX_PROTOCOL_FEE_BP, TIMELOCK_DELAY_SECONDS};

/// A timelocked administrative operation queued for future execution.
///
/// Created by [`RaffleFactory::set_config`] and stored under
/// [`DataKey::PendingOp`] until either executed by
/// [`RaffleFactory::execute_config_change`] or cancelled by
/// [`RaffleFactory::cancel_config_change`].
///
/// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) — `AdminOpProposed`,
/// `AdminOpExecuted`, `AdminOpCancelled`.
#[derive(Clone)]
#[contracttype]
pub struct PendingOp {
    /// The operation payload to apply once the timelock elapses.
    pub op: AdminOp,
    /// Unix timestamp (seconds) at which the operation becomes executable.
    /// Equals the ledger timestamp at proposal time plus
    /// [`TIMELOCK_DELAY_SECONDS`] (48 hours).
    pub effective_timestamp: u64,
    /// Address of the admin who proposed this operation.
    pub proposed_by: Address,
}

/// A periodic state snapshot recording factory health at a milestone raffle
/// count.
///
/// A checkpoint is automatically created every
/// [`CHECKPOINT_INTERVAL`] (1 000) total raffles. The
/// `aggregate_hash` is a SHA-256 digest of `raffle_count ‖ ledger_sequence ‖
/// ledger_timestamp`, giving indexers a compact, tamper-evident anchor.
///
/// Retrieve checkpoints with [`RaffleFactory::get_checkpoint`] and
/// [`RaffleFactory::get_latest_checkpoint_index`].
///
/// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) — `CheckpointCreated`.
#[derive(Clone)]
#[contracttype]
pub struct StateCheckpoint {
    /// Sequential 1-based checkpoint index (`raffle_count / CHECKPOINT_INTERVAL`).
    pub index: u32,
    /// Total number of raffles created when this checkpoint was taken.
    pub raffle_count: u32,
    /// Ledger timestamp (Unix seconds) when this checkpoint was recorded.
    pub ledger_timestamp: u64,
    /// SHA-256 digest of `raffle_count ‖ ledger_sequence ‖ ledger_timestamp`.
    /// Used by off-chain monitors to detect storage tampering.
    pub aggregate_hash: BytesN<32>,
}

/// Persistent storage keys used by the factory contract.
///
/// Each variant maps to exactly one storage slot, keeping reads and writes
/// O(1). The stable-map design (`RaffleById` / `NextRaffleId`) means that
/// adding or removing a raffle never touches any other raffle's slot.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Flag set to `true` after the first successful [`RaffleFactory::init_factory`]
    /// call. Guards against re-initialization.
    Initialized,
    /// Current admin [`Address`]. Updated by a completed two-step transfer or
    /// directly by [`RaffleFactory::accept_factory_admin`].
    Admin,
    /// Stable map: stable_id (u32) → raffle Address.
    /// Replaces the old RaffleInstances Vec — each entry is an independent
    /// storage slot so reads and writes are always O(1).
    RaffleById(u32),
    /// Monotonic counter: the stable_id that will be assigned to the *next*
    /// raffle.  Starts at 0 and is never decremented.
    NextRaffleId,
    /// Number of live (non-tombstoned) raffles.  Used for stats only.
    RaffleCount,
    /// WASM hash of the raffle-instance contract deployed by
    /// [`RaffleFactory::create_raffle`].
    InstanceWasmHash,
    /// Protocol fee in basis points applied to every new raffle instance.
    ProtocolFeeBP,
    /// Treasury [`Address`] that receives protocol fees.
    Treasury,
    /// Boolean pause flag stored in instance storage. When `true`,
    /// [`RaffleFactory::create_raffle`] is blocked.
    Paused,
    /// Pending admin [`Address`] set by
    /// [`RaffleFactory::transfer_factory_admin`]; cleared on acceptance or
    /// cancellation.
    PendingAdmin,
    /// Timelocked operation keyed by its auto-incrementing `op_id`.
    PendingOp(u32),
    /// Monotonic counter that assigns unique IDs to pending operations.
    OpCounter,
    /// State checkpoint keyed by its sequential index.
    Checkpoint(u32),
    /// Index of the most recently written [`StateCheckpoint`].
    LatestCheckpointIndex,
    /// Cumulative count of all raffles ever created (never decremented).
    /// Used as input to the checkpoint trigger.
    TotalRafflesCreated,
    /// Per-address flag (`true`) recording that an address has participated.
    /// Used to maintain the unique-participant count without double-counting.
    UniqueParticipant(Address),
    /// Running count of unique participant addresses across all raffles.
    TotalUniqueParticipants,
    /// Minimum seconds between raffle creations for non-whitelisted creators.
    /// Defaults to 300 s when absent.
    MinCreationDelay,
    /// Unix timestamp of the most recent successful raffle creation for each
    /// non-whitelisted creator address. Used by the rate limiter.
    LastCreationTime(Address),
    /// Whitelist flag for partner addresses. When `true`, the address bypasses
    /// the creation rate limiter entirely.
    WhitelistedPartner(Address),
    /// Cumulative ticket-sale volume denominated in a specific asset. Updated
    /// by [`RaffleFactory::record_volume`] on every ticket purchase.
    TotalVolumePerAsset(Address),
    /// Kept for test-only address generation; not used for indexing.
    RaffleInstancesCount,
    /// Per-creator raffle index: creator Address → Vec<Address> of raffle addresses.
    /// Appended to on every successful `create_raffle`.
    CreatorRaffles(Address),
    /// Per-category raffle index (#439): category String → Vec<Address> of raffle
    /// addresses. Appended to on every successful `create_raffle` whose config
    /// carries a category, enabling `get_raffles_by_category` queries without an
    /// off-chain indexer.
    CategoryRaffles(soroban_sdk::String),
    /// Approved-oracle allowlist entry: Address → bool.
    /// When `true` the oracle is approved for use in External-mode raffles.
    ApprovedOracle(Address),
    /// When `true`, `create_raffle` skips the oracle allowlist check.
    /// Intended for test deployments; must be set at init time or via admin.
    PermissionlessOracles,
}

/// A read-only snapshot of key factory metrics returned by
/// [`RaffleFactory::get_protocol_stats`].
#[derive(Clone)]
#[contracttype]
pub struct ProtocolStats {
    /// Cumulative number of raffle instances ever created by this factory.
    pub total_raffles_created: u32,
    /// Current protocol fee in basis points (100 = 1 %).
    pub protocol_fee_bp: u32,
    /// Whether the factory is currently paused (`create_raffle` is blocked).
    pub paused: bool,
    /// Number of unique participant addresses tracked across all raffles.
    pub total_unique_participants: u32,
}

/// Errors returned by the factory contract.
///
/// Each variant maps to a unique `u32` discriminant so that Stellar clients and
/// off-chain integrations can match on numeric codes without parsing strings.
/// See [`docs/ERRORS.md`](../../../docs/ERRORS.md) for the complete reference.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    /// `init_factory` was called on an already-initialized contract. Code 1.
    AlreadyInitialized = 1,
    /// Caller is not the admin or the operation requires admin authorization.
    /// Code 2.
    NotAuthorized = 2,
    /// The factory is paused; raffle creation is blocked until unpaused.
    /// Code 3.
    ContractPaused = 3,
    /// A supplied parameter is out of range or otherwise invalid (e.g., fee
    /// exceeds [`MAX_PROTOCOL_FEE_BP`], zero/self address). Code 4.
    InvalidParameters = 4,
    /// The requested raffle stable-ID does not map to an existing contract.
    /// Code 5.
    RaffleNotFound = 5,
    /// A two-step admin transfer is already in progress; the current proposal
    /// must be accepted or cancelled before a new one can be opened. Code 11.
    AdminTransferPending = 11,
    /// `accept_factory_admin` was called but there is no pending transfer.
    /// Code 12.
    NoPendingTransfer = 12,
    /// A non-whitelisted creator attempted to create a raffle before the
    /// [`MinCreationDelay`](DataKey::MinCreationDelay) window elapsed. Code 13.
    RateLimitExceeded = 13,
    /// `execute_config_change` or `cancel_config_change` was called with an
    /// `op_id` that has no pending operation. Code 14.
    NoPendingOp = 14,
    /// `execute_config_change` was called before `effective_timestamp` was
    /// reached. Code 15.
    TimelockNotElapsed = 15,
    /// `clean_old_raffle` was called with an ID that is not in the stable-map
    /// (never assigned or already tombstoned). Code 16.
    InvalidRaffleId = 16,
    /// Reserved for future use — a raffle does not meet eligibility criteria
    /// for the requested operation. Code 17.
    RaffleNotEligible = 17,
    /// A `checked_add` overflow occurred while accumulating volume. Code 18.
    ArithmeticOverflow = 18,
    /// `create_raffle` could not read the treasury address (factory not fully
    /// initialized). Code 19.
    TreasuryNotSet = 19,
    /// `create_raffle` was called with an oracle address not in the approved
    /// allowlist, while the factory is in registry-enforcing mode. Code 20.
    OracleNotApproved = 20,
}

#[contract]
pub struct RaffleFactory;

raffle_shared::impl_require_admin!(ContractError, ContractError::NotAuthorized);
raffle_shared::impl_require_not_paused!(
    ContractError,
    ContractError::ContractPaused,
    require_factory_not_paused
);

fn maybe_create_checkpoint(env: &Env, raffle_count: u32) {
    if raffle_count == 0 || !raffle_count.is_multiple_of(CHECKPOINT_INTERVAL) {
        return;
    }

    let index = raffle_count / CHECKPOINT_INTERVAL;
    let ledger_timestamp = env.ledger().timestamp();
    let ledger_sequence = env.ledger().sequence();

    let mut input = Bytes::new(env);
    input.extend_from_array(&raffle_count.to_be_bytes());
    input.extend_from_array(&ledger_sequence.to_be_bytes());
    input.extend_from_array(&ledger_timestamp.to_be_bytes());

    let aggregate_hash = env.crypto().sha256(&input);

    let checkpoint = StateCheckpoint {
        index,
        raffle_count,
        ledger_timestamp,
        aggregate_hash: aggregate_hash.clone().into(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::Checkpoint(index), &checkpoint);
    env.storage()
        .persistent()
        .set(&DataKey::LatestCheckpointIndex, &index);

    events::CheckpointCreated {
        index,
        raffle_count,
        ledger_timestamp,
        aggregate_hash: aggregate_hash.into(),
    }
    .publish(env);
}

/// Validate that an address is usable for a privileged role (admin/treasury).
///
/// Rejects the zero contract address (all-zero 32-byte hash) and the factory's
/// own address to prevent a self-referential admin or treasury that would brick
/// the contract.  Account (keypair) addresses are always accepted.
fn require_valid_role_address(env: &Env, address: &Address) -> Result<(), ContractError> {
    #[cfg(not(test))]
    if !address.exists() {
        return Err(ContractError::InvalidParameters);
    }
    // In test mode the exists() check is skipped, but we still reject the
    // all-zeros contract id (the "zero address") explicitly.
    #[cfg(test)]
    {
        use soroban_sdk::String;
        const ZERO_CONTRACT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
        let zero = Address::from_string(&String::from_str(env, ZERO_CONTRACT));
        if *address == zero {
            return Err(ContractError::InvalidParameters);
        }
    }
    if *address == env.current_contract_address() {
        return Err(ContractError::InvalidParameters);
    }
    Ok(())
}

#[contractimpl]
impl RaffleFactory {
    /// Initialize the factory contract.
    ///
    /// Must be called exactly once immediately after deployment. Subsequent
    /// calls return [`ContractError::AlreadyInitialized`].
    ///
    /// # Parameters
    ///
    /// - `admin` — Privileged address that may call admin-only functions.
    ///   Must not be the zero contract address or the factory's own address.
    /// - `wasm_hash` — WASM hash of the raffle-instance contract that will be
    ///   deployed by [`create_raffle`](Self::create_raffle).
    /// - `protocol_fee_bp` — Initial protocol fee in basis points
    ///   (max [`MAX_PROTOCOL_FEE_BP`] = 2 000, i.e. 20 %).
    /// - `treasury` — Address that receives protocol fees. Must not be the
    ///   zero contract address or the factory's own address.
    ///
    /// # Errors
    ///
    /// - [`ContractError::AlreadyInitialized`] — factory was already
    ///   initialized.
    /// - [`ContractError::InvalidParameters`] — `protocol_fee_bp` exceeds the
    ///   cap, or `admin`/`treasury` is the zero address or the factory itself.
    ///
    /// # Events
    ///
    /// Emits [`events::FactoryInitialized`] on success.
    ///
    /// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) —
    /// `FactoryInitialized`.
    pub fn init_factory(
        env: Env,
        admin: Address,
        wasm_hash: BytesN<32>,
        protocol_fee_bp: u32,
        treasury: Address,
    ) -> Result<(), ContractError> {
        if env.storage().persistent().has(&DataKey::Initialized) {
            return Err(ContractError::AlreadyInitialized);
        }
        if protocol_fee_bp > MAX_PROTOCOL_FEE_BP {
            return Err(ContractError::InvalidParameters);
        }
        require_valid_role_address(&env, &admin)?;
        require_valid_role_address(&env, &treasury)?;
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::InstanceWasmHash, &wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &treasury);
        env.storage().persistent().set(&DataKey::Initialized, &true);

        events::FactoryInitialized {
            admin,
            protocol_fee_bp,
            treasury,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Propose a protocol-configuration change under a 48-hour timelock.
    ///
    /// The change is **not** applied immediately. It is stored as a
    /// [`PendingOp`] and becomes executable only after
    /// [`TIMELOCK_DELAY_SECONDS`] (48 hours) have elapsed. Call
    /// [`execute_config_change`](Self::execute_config_change) with the
    /// returned `op_id` to apply it, or
    /// [`cancel_config_change`](Self::cancel_config_change) to discard it.
    ///
    /// # Auth
    ///
    /// Requires authorization from the current admin address.
    ///
    /// # Parameters
    ///
    /// - `protocol_fee_bp` — New protocol fee in basis points (max
    ///   [`MAX_PROTOCOL_FEE_BP`] = 2 000).
    /// - `treasury` — New treasury address. Must not be the zero contract
    ///   address or the factory's own address.
    ///
    /// # Returns
    ///
    /// The auto-incremented `op_id` that identifies this pending operation.
    ///
    /// # Errors
    ///
    /// - [`ContractError::NotAuthorized`] — caller is not the admin.
    /// - [`ContractError::InvalidParameters`] — fee exceeds cap or treasury
    ///   address is invalid.
    ///
    /// # Events
    ///
    /// Emits [`events::AdminOpProposed`] on success.
    ///
    /// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) —
    /// `AdminOpProposed`.
    pub fn set_config(
        env: Env,
        protocol_fee_bp: u32,
        treasury: Address,
    ) -> Result<u32, ContractError> {
        let admin = require_admin(&env)?;
        if protocol_fee_bp > MAX_PROTOCOL_FEE_BP {
            return Err(ContractError::InvalidParameters);
        }
        require_valid_role_address(&env, &treasury)?;

        let op_id = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::OpCounter)
            .unwrap_or(0)
            .saturating_add(1);

        env.storage().persistent().set(&DataKey::OpCounter, &op_id);

        let effective_timestamp = env.ledger().timestamp() + TIMELOCK_DELAY_SECONDS;
        let op = AdminOp::SetConfig(protocol_fee_bp, treasury.clone());
        let pending = PendingOp {
            op: op.clone(),
            effective_timestamp,
            proposed_by: admin.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::PendingOp(op_id), &pending);

        events::AdminOpProposed {
            op_id,
            op,
            effective_timestamp,
            proposed_by: admin,
        }
        .publish(&env);

        Ok(op_id)
    }

    /// Apply a timelocked configuration change that has passed its delay.
    ///
    /// Reads the [`PendingOp`] stored under `op_id`, verifies the timelock has
    /// elapsed, applies the change to persistent storage, and removes the
    /// pending entry.
    ///
    /// Supported operations:
    /// - [`AdminOp::SetConfig`] — updates `protocol_fee_bp` and `treasury`.
    /// - [`AdminOp::UpdateWasmHash`] — rotates the instance WASM hash used for
    ///   future deploys.
    ///
    /// # Auth
    ///
    /// Requires authorization from the current admin address.
    ///
    /// # Parameters
    ///
    /// - `op_id` — Identifier returned by [`set_config`](Self::set_config).
    ///
    /// # Errors
    ///
    /// - [`ContractError::NotAuthorized`] — caller is not the admin.
    /// - [`ContractError::NoPendingOp`] — no pending operation for `op_id`.
    /// - [`ContractError::TimelockNotElapsed`] — the 48-hour delay has not
    ///   passed yet.
    /// - [`ContractError::InvalidParameters`] — embedded parameters are
    ///   invalid (fee cap, address checks).
    ///
    /// # Events
    ///
    /// Emits [`events::AdminOpExecuted`] on success.
    ///
    /// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) —
    /// `AdminOpExecuted`.
    pub fn execute_config_change(env: Env, op_id: u32) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        let pending: PendingOp = env
            .storage()
            .persistent()
            .get(&DataKey::PendingOp(op_id))
            .ok_or(ContractError::NoPendingOp)?;

        if env.ledger().timestamp() < pending.effective_timestamp {
            return Err(ContractError::TimelockNotElapsed);
        }

        match pending.op.clone() {
            AdminOp::SetConfig(protocol_fee_bp, treasury) => {
                if protocol_fee_bp > MAX_PROTOCOL_FEE_BP {
                    return Err(ContractError::InvalidParameters);
                }
                require_valid_role_address(&env, &treasury)?;
                env.storage()
                    .persistent()
                    .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
                env.storage()
                    .persistent()
                    .set(&DataKey::Treasury, &treasury);
            }
            AdminOp::UpdateWasmHash(new_hash) => {
                env.storage()
                    .persistent()
                    .set(&DataKey::InstanceWasmHash, &new_hash);
            }
            AdminOp::ApproveOracle(oracle) => {
                env.storage()
                    .persistent()
                    .set(&DataKey::ApprovedOracle(oracle.clone()), &true);
                events::OracleApproved {
                    oracle,
                    approved_by: admin.clone(),
                    timestamp: env.ledger().timestamp(),
                }
                .publish(&env);
            }
            AdminOp::RemoveOracle(oracle) => {
                env.storage()
                    .persistent()
                    .remove(&DataKey::ApprovedOracle(oracle.clone()));
                events::OracleRemoved {
                    oracle,
                    removed_by: admin.clone(),
                    timestamp: env.ledger().timestamp(),
                }
                .publish(&env);
            }
        }

        env.storage()
            .persistent()
            .remove(&DataKey::PendingOp(op_id));

        events::AdminOpExecuted {
            op_id,
            op: pending.op,
            executed_by: admin,
            executed_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Cancel a pending timelocked configuration change.
    ///
    /// Removes the [`PendingOp`] stored under `op_id` without applying it.
    /// The operation cannot be recovered after cancellation.
    ///
    /// # Auth
    ///
    /// Requires authorization from the current admin address.
    ///
    /// # Parameters
    ///
    /// - `op_id` — Identifier returned by [`set_config`](Self::set_config).
    ///
    /// # Errors
    ///
    /// - [`ContractError::NotAuthorized`] — caller is not the admin.
    /// - [`ContractError::NoPendingOp`] — no pending operation for `op_id`.
    ///
    /// # Events
    ///
    /// Emits [`events::AdminOpCancelled`] on success.
    ///
    /// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) —
    /// `AdminOpCancelled`.
    pub fn cancel_config_change(env: Env, op_id: u32) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        if !env.storage().persistent().has(&DataKey::PendingOp(op_id)) {
            return Err(ContractError::NoPendingOp);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::PendingOp(op_id));

        events::AdminOpCancelled {
            op_id,
            cancelled_by: admin,
            cancelled_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Return the pending operation for `op_id`, or `None` if it has been
    /// executed, cancelled, or never created.
    pub fn get_pending_op(env: Env, op_id: u32) -> Option<PendingOp> {
        env.storage().persistent().get(&DataKey::PendingOp(op_id))
    }

    /// Return the current operation counter value.
    ///
    /// The next call to [`set_config`](Self::set_config) will produce an
    /// `op_id` equal to this value plus one.
    pub fn get_op_counter(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::OpCounter)
            .unwrap_or(0u32)
    }

    /// Deploy a new raffle-instance contract and register it with the factory.
    ///
    /// This is the primary entry point for raffle creators. The function:
    ///
    /// 1. Checks the factory is not paused.
    /// 2. Enforces the creation rate limiter for non-whitelisted creators
    ///    (default 300 s cooldown, configurable via
    ///    [`set_creation_delay`](Self::set_creation_delay)).
    /// 3. Injects the current `protocol_fee_bp` and `treasury` into the config.
    /// 4. Deploys a new raffle-instance WASM contract (address derived from
    ///    `creator` + `description` SHA-256 salt).
    /// 5. Calls `init` on the deployed instance.
    /// 6. Registers the address in the O(1) stable-ID map and the per-creator
    ///    index. If the config declares a `category`, also appends to the
    ///    per-category index.
    /// 7. Triggers a [`StateCheckpoint`] every
    ///    [`CHECKPOINT_INTERVAL`] total raffles.
    ///
    /// # Auth
    ///
    /// Requires authorization from `creator`.
    ///
    /// # Parameters
    ///
    /// - `creator` — Address that will own the raffle and receive any creator
    ///   privileges within the instance.
    /// - `config` — Full raffle configuration. `protocol_fee_bp` and
    ///   `treasury_address` fields are **overwritten** by the factory's stored
    ///   values regardless of what the caller provides.
    ///
    /// # Returns
    ///
    /// The [`Address`] of the newly deployed raffle-instance contract.
    ///
    /// # Errors
    ///
    /// - [`ContractError::ContractPaused`] — factory is paused.
    /// - [`ContractError::RateLimitExceeded`] — non-whitelisted creator is
    ///   within the cooldown window (also emits [`events::CreationRateLimited`]).
    /// - [`ContractError::TreasuryNotSet`] — factory treasury address not
    ///   initialized.
    /// - [`ContractError::NotAuthorized`] — factory admin address missing
    ///   (should not occur after `init_factory`).
    /// - [`ContractError::InvalidParameters`] — WASM hash not set (production
    ///   only).
    ///
    /// # Events
    ///
    /// - [`events::CreationRateLimited`] when a non-whitelisted creator is
    ///   rate-limited (returned together with
    ///   [`ContractError::RateLimitExceeded`]).
    /// - The deployed instance emits `RaffleCreated` on its own `init` call.
    ///
    /// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) —
    /// `CreationRateLimited`.
    pub fn create_raffle(
        env: Env,
        creator: Address,
        config: RaffleConfig,
    ) -> Result<Address, ContractError> {
        creator.require_auth();
        require_factory_not_paused(&env)?;

        let is_whitelisted = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedPartner(creator.clone()))
            .unwrap_or(false);

        if !is_whitelisted {
            let now = env.ledger().timestamp();
            let min_delay = env
                .storage()
                .persistent()
                .get(&DataKey::MinCreationDelay)
                .unwrap_or(300);

            let last_creation: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::LastCreationTime(creator.clone()))
                .unwrap_or(0);

            if now < last_creation + min_delay {
                let unlock_timestamp = last_creation + min_delay;
                events::CreationRateLimited {
                    creator: creator.clone(),
                    unlock_timestamp,
                    timestamp: now,
                }
                .publish(&env);
                return Err(ContractError::RateLimitExceeded);
            }

            env.storage()
                .persistent()
                .set(&DataKey::LastCreationTime(creator.clone()), &now);
        }

        let protocol_fee_bp: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBP)
            .unwrap_or(0);
        let treasury: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Treasury)
            .ok_or(ContractError::TreasuryNotSet)?;

        let mut final_config = config;
        final_config.protocol_fee_bp = protocol_fee_bp;
        final_config.treasury_address = Some(treasury);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;
        let factory_address = env.current_contract_address();

        // --- Oracle allowlist enforcement ---
        // If the factory is NOT in permissionless mode, External-mode raffles
        // must reference an oracle that the admin has explicitly approved.
        if final_config.randomness_source == raffle_shared::RandomnessSource::External {
            let permissionless: bool = env
                .storage()
                .persistent()
                .get(&DataKey::PermissionlessOracles)
                .unwrap_or(false);
            if !permissionless {
                let oracle_addr = final_config
                    .oracle_address
                    .as_ref()
                    .ok_or(ContractError::InvalidParameters)?;
                let approved: bool = env
                    .storage()
                    .persistent()
                    .get(&DataKey::ApprovedOracle(oracle_addr.clone()))
                    .unwrap_or(false);
                if !approved {
                    return Err(ContractError::OracleNotApproved);
                }
            }
        }

        #[cfg(not(test))]
        let raffle_address = {
            let wasm_hash: BytesN<32> = env
                .storage()
                .persistent()
                .get(&DataKey::InstanceWasmHash)
                .ok_or(ContractError::InvalidParameters)?;
            let salt = env
                .crypto()
                .sha256(&(creator.clone(), final_config.description.clone()).to_xdr(&env));
            env.deployer()
                .with_address(factory_address.clone(), salt)
                .deploy_v2(wasm_hash, ())
        };

        #[cfg(test)]
        let raffle_address = {
            let mut count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::RaffleInstancesCount)
                .unwrap_or(0);
            count += 1;
            env.storage()
                .persistent()
                .set(&DataKey::RaffleInstancesCount, &count);

            let mut id = Address::generate(&env);
            for _ in 0..count {
                id = Address::generate(&env);
            }
            env.register_at(&id, raffle_instance::Contract, ());
            id
        };

        let category = final_config.category.clone();
        env.invoke_contract::<()>(
            &raffle_address,
            &Symbol::new(&env, "init"),
            (factory_address, admin, creator.clone(), final_config).into_val(&env),
        );

        // --- O(1) stable-map registration ---
        // Assign the next stable ID and write a single entry.  No Vec is
        // deserialised or reserialised; each raffle occupies its own storage
        // slot, so cost is constant regardless of how many raffles exist.
        let stable_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleById(stable_id), &raffle_address);
        env.storage()
            .persistent()
            .set(&DataKey::NextRaffleId, &(stable_id.saturating_add(1)));

        // --- per-creator index ---
        // Append the new raffle address to the creator's list so callers can
        // query all raffles for a given creator without scanning the full list.
        let mut creator_raffles: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorRaffles(creator.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        creator_raffles.push_back(raffle_address.clone());
        env.storage()
            .persistent()
            .set(&DataKey::CreatorRaffles(creator.clone()), &creator_raffles);

        // --- per-category index (#439) ---
        // If the raffle declares a category, append its address to that
        // category's list so `get_raffles_by_category` can serve paginated
        // results directly from on-chain state.  The category was validated
        // (length + charset) by the instance's `init`, invoked above.
        if let Some(ref category) = category {
            let mut cat_raffles: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::CategoryRaffles(category.clone()))
                .unwrap_or_else(|| Vec::new(&env));
            cat_raffles.push_back(raffle_address.clone());
            env.storage()
                .persistent()
                .set(&DataKey::CategoryRaffles(category.clone()), &cat_raffles);
        }

        // Increment the live-count for stats.
        let live_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32)
            .saturating_add(1);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleCount, &live_count);

        let mut count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRafflesCreated)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::TotalRafflesCreated, &count);

        maybe_create_checkpoint(&env, count);

        Ok(raffle_address)
    }

    /// Return a snapshot of key factory metrics.
    ///
    /// This is a read-only call with no auth requirement. All fields default to
    /// zero/false when the factory has just been initialized and no raffles have
    /// been created.
    pub fn get_protocol_stats(env: Env) -> ProtocolStats {
        let total_raffles_created: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRafflesCreated)
            .unwrap_or(0);
        let protocol_fee_bp: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBP)
            .unwrap_or(0);
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        let total_unique_participants: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalUniqueParticipants)
            .unwrap_or(0);

        ProtocolStats {
            total_raffles_created,
            protocol_fee_bp,
            paused,
            total_unique_participants,
        }
    }

    /// O(1) direct lookup of a raffle address by its stable ID.
    /// Returns `None` if the ID was never assigned or has been cleaned up.
    pub fn get_raffle_by_id(env: Env, raffle_id: u32) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::RaffleById(raffle_id))
    }

    /// Returns the stable ID that will be assigned to the next raffle.
    /// IDs in [0, next_raffle_id) have been assigned at least once.
    pub fn get_next_raffle_id(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32)
    }

    /// Returns the current count of live (non-tombstoned) raffles.
    pub fn get_raffle_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32)
    }

    /// Return the cumulative ticket-sale volume for a specific `asset` token.
    ///
    /// Returns `0` when no volume has been recorded for `asset`. No auth
    /// required.
    pub fn get_total_volume(env: Env, asset: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalVolumePerAsset(asset))
            .unwrap_or(0)
    }

    /// Accumulate `amount` into the running volume counter for `asset`.
    ///
    /// Called by raffle instances on every successful ticket purchase to
    /// update the factory-level per-asset volume metric. This is an internal
    /// cross-contract call — end users do not call it directly.
    ///
    /// # Auth
    ///
    /// No explicit admin check; the instance is trusted by the factory because
    /// the factory deployed it. The instance itself validates that the caller
    /// is the ticket buyer.
    ///
    /// # Errors
    ///
    /// - [`ContractError::ArithmeticOverflow`] — adding `amount` to the
    ///   current total would exceed `i128::MAX`.
    pub fn record_volume(env: Env, asset: Address, amount: i128) -> Result<(), ContractError> {
        let total_volume: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalVolumePerAsset(asset.clone()))
            .unwrap_or(0);
        let total_volume = total_volume
            .checked_add(amount)
            .ok_or(ContractError::ArithmeticOverflow)?;
        env.storage()
            .persistent()
            .set(&DataKey::TotalVolumePerAsset(asset), &total_volume);
        Ok(())
    }

    /// Return the current admin address.
    ///
    /// # Errors
    ///
    /// - [`ContractError::NotAuthorized`] — admin key is missing (factory not
    ///   initialized).
    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)
    }

    /// Return a paginated slice of all live raffle addresses.
    ///
    /// Iterates over the stable-ID space `[offset, offset + limit)` and
    /// returns only slots that still hold a live address (tombstoned entries
    /// from [`clean_old_raffle`](Self::clean_old_raffle) are silently skipped).
    /// Each iteration step is a single O(1) storage lookup.
    ///
    /// # Parameters
    ///
    /// - `params.offset` — First stable-ID to include. Acts as a cursor into
    ///   the ever-increasing ID space (not the live-raffle count).
    /// - `params.limit` — Maximum results per page. Clamped to
    ///   `[1, MAX_PAGE_LIMIT]`; `0` uses `DEFAULT_PAGE_LIMIT` (100).
    ///
    /// # Returns
    ///
    /// A [`PageResultRaffles`] whose `total` field reflects the number of
    /// **live** raffles (not the total IDs ever assigned), and `has_more` is
    /// `true` when the stable-ID space extends beyond the returned window.
    pub fn get_raffles_page(env: Env, params: PaginationParams) -> PageResultRaffles {
        // `NextRaffleId` is the exclusive upper bound on all ever-assigned IDs.
        // It equals the total number of raffles ever created (including any that
        // have been cleaned up / tombstoned).
        let next_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32);

        let lim = effective_limit(params.limit);
        let offset = params.offset;

        // `total` here is the live-raffle count (tombstoned entries excluded),
        // reported to the caller for UI pagination purposes.
        let total: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32);

        if offset >= next_id {
            return PageResultRaffles {
                items: Vec::new(&env),
                total,
                has_more: false,
            };
        }

        // Walk the stable ID space [offset, offset + lim) and collect only
        // slots that still hold a live address (non-tombstoned).  Each read
        // is a single O(1) storage lookup; the loop is bounded by `lim`.
        let end = offset.saturating_add(lim).min(next_id);
        let mut items: Vec<Address> = Vec::new(&env);
        for id in offset..end {
            if let Some(addr) = env
                .storage()
                .persistent()
                .get::<_, Address>(&DataKey::RaffleById(id))
            {
                items.push_back(addr);
            }
        }

        let has_more = end < next_id;
        PageResultRaffles {
            items,
            total,
            has_more,
        }
    }

    /// Return a paginated list of raffle addresses created by `creator`.
    ///
    /// `params.offset` is an index into the creator's personal raffle list
    /// (not the global stable-ID space).  `params.limit` is clamped by
    /// `effective_limit` (1–200, default 100).
    pub fn get_raffles_by_creator(
        env: Env,
        creator: Address,
        params: PaginationParams,
    ) -> PageResultRaffles {
        let creator_raffles: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorRaffles(creator))
            .unwrap_or_else(|| Vec::new(&env));

        let total = creator_raffles.len();
        let lim = effective_limit(params.limit);
        let offset = params.offset;

        if offset >= total {
            return PageResultRaffles {
                items: Vec::new(&env),
                total,
                has_more: false,
            };
        }

        let end = offset.saturating_add(lim).min(total);
        let mut items: Vec<Address> = Vec::new(&env);
        for i in offset..end {
            if let Some(addr) = creator_raffles.get(i) {
                items.push_back(addr);
            }
        }

        let has_more = end < total;
        PageResultRaffles {
            items,
            total,
            has_more,
        }
    }

    /// Return a paginated list of raffle addresses tagged with `category` (#439).
    ///
    /// `params.offset` is an index into the category's raffle list.
    /// `params.limit` is clamped by `effective_limit` (1–200, default 100).
    /// An unknown category simply yields an empty page.
    pub fn get_raffles_by_category(
        env: Env,
        category: soroban_sdk::String,
        params: PaginationParams,
    ) -> PageResultRaffles {
        let category_raffles: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CategoryRaffles(category))
            .unwrap_or_else(|| Vec::new(&env));

        let total = category_raffles.len();
        let lim = effective_limit(params.limit);
        let offset = params.offset;

        if offset >= total {
            return PageResultRaffles {
                items: Vec::new(&env),
                total,
                has_more: false,
            };
        }

        let end = offset.saturating_add(lim).min(total);
        let mut items: Vec<Address> = Vec::new(&env);
        for i in offset..end {
            if let Some(addr) = category_raffles.get(i) {
                items.push_back(addr);
            }
        }

        let has_more = end < total;
        PageResultRaffles {
            items,
            total,
            has_more,
        }
    }

    /// Pause the factory, blocking new raffle creation.
    ///
    /// While paused, [`create_raffle`](Self::create_raffle) returns
    /// [`ContractError::ContractPaused`]. All other reads and admin operations
    /// remain available.
    ///
    /// # Auth
    ///
    /// Requires authorization from the current admin address.
    ///
    /// # Errors
    ///
    /// - [`ContractError::NotAuthorized`] — caller is not the admin.
    ///
    /// # Events
    ///
    /// Emits [`events::ContractPaused`].
    ///
    /// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) — `ContractPaused`.
    pub fn pause_factory(env: Env) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);

        events::ContractPaused {
            paused_by: admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn unpause_factory(env: Env) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);

        events::ContractUnpaused {
            unpaused_by: admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn is_factory_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn transfer_factory_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        if new_admin == admin {
            env.storage().persistent().remove(&DataKey::PendingAdmin);
            return Ok(());
        }

        require_valid_role_address(&env, &new_admin)?;

        if env.storage().persistent().has(&DataKey::PendingAdmin) {
            return Err(ContractError::AdminTransferPending);
        }

        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);

        events::AdminTransferProposed {
            current_admin: admin,
            proposed_admin: new_admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn accept_factory_admin(env: Env) -> Result<(), ContractError> {
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .ok_or(ContractError::NoPendingTransfer)?;
        pending.require_auth();

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;

        env.storage().persistent().set(&DataKey::Admin, &pending);
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        events::AdminTransferAccepted {
            old_admin,
            new_admin: pending,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn get_checkpoint(env: Env, index: u32) -> Option<StateCheckpoint> {
        env.storage().persistent().get(&DataKey::Checkpoint(index))
    }

    pub fn get_latest_checkpoint_index(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestCheckpointIndex)
            .unwrap_or(0u32)
    }

    pub fn sync_admin(env: Env, instance_address: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.invoke_contract::<()>(
            &instance_address,
            &Symbol::new(&env, "set_admin"),
            (admin,).into_val(&env),
        );
        Ok(())
    }

    pub fn pause_instance(env: Env, instance_address: Address) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.invoke_contract::<()>(
            &instance_address,
            &Symbol::new(&env, "pause"),
            ().into_val(&env),
        );
        Ok(())
    }

    pub fn unpause_instance(env: Env, instance_address: Address) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.invoke_contract::<()>(
            &instance_address,
            &Symbol::new(&env, "unpause"),
            ().into_val(&env),
        );
        Ok(())
    }

    pub fn track_participant(env: Env, participant: Address) -> Result<(), ContractError> {
        participant.require_auth();

        let key = DataKey::UniqueParticipant(participant.clone());
        if !env.storage().persistent().has(&key) {
            env.storage().persistent().set(&key, &true);
            let mut count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalUniqueParticipants)
                .unwrap_or(0);
            count += 1;
            env.storage()
                .persistent()
                .set(&DataKey::TotalUniqueParticipants, &count);
        }
        Ok(())
    }

    pub fn get_unique_participants(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalUniqueParticipants)
            .unwrap_or(0)
    }

    pub fn get_raffle_fairness_data(
        env: Env,
        raffle_id: Address,
    ) -> Result<FairnessData, ContractError> {
        Ok(env.invoke_contract::<FairnessData>(
            &raffle_id,
            &Symbol::new(&env, "get_fairness_data"),
            ().into_val(&env),
        ))
    }

    pub fn set_creation_delay(env: Env, delay_seconds: u64) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::MinCreationDelay, &delay_seconds);
        Ok(())
    }

    pub fn set_whitelist_status(
        env: Env,
        partner: Address,
        status: bool,
    ) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedPartner(partner), &status);
        Ok(())
    }

    /// Standard Soroban upgrade entry point for the factory contract WASM.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        events::FactoryUpgraded {
            admin,
            new_wasm_hash,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Sweep tokens accidentally sent to the factory contract.
    pub fn rescue_tokens(
        env: Env,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidParameters);
        }

        let token_client = token::Client::new(&env, &token);
        let _ = token_client
            .try_transfer(&env.current_contract_address(), &recipient, &amount)
            .map_err(|_| ContractError::InvalidParameters)?;

        events::FactoryTokensRescued {
            rescued_by: admin,
            token,
            recipient,
            amount,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn clean_old_raffle(env: Env, raffle_id: u32) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        // Look up the raffle by its stable ID.  A missing entry means the ID
        // was never assigned or has already been cleaned up.
        let raffle_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleById(raffle_id))
            .ok_or(ContractError::InvalidRaffleId)?;

        env.invoke_contract::<()>(
            &raffle_address,
            &Symbol::new(&env, "wipe_storage"),
            ().into_val(&env),
        );

        // Tombstone: remove the stable-map entry so the slot is freed and
        // `get_raffles_page` will skip it.  The stable_id is never reused so
        // other IDs are completely unaffected — no shifting, no reindexing.
        env.storage()
            .persistent()
            .remove(&DataKey::RaffleById(raffle_id));

        // Decrement the live count (floor at 0 for safety).
        let live_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleCount, &live_count.saturating_sub(1));

        events::RaffleCleanedUp {
            raffle_address,
            cleaned_by: admin,
            finish_time: 0,
            cleaned_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Return `true` if `oracle` is in the approved-oracle allowlist.
    pub fn is_oracle_approved(env: Env, oracle: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::ApprovedOracle(oracle))
            .unwrap_or(false)
    }

    /// Return `true` if the factory is in permissionless-oracle mode
    /// (oracle allowlist not enforced — intended for test deployments).
    pub fn is_permissionless_oracles(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::PermissionlessOracles)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raffle_shared::{RandomnessSource, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
    use soroban_sdk::{String, Vec as SdkVec};

    fn setup_factory(env: &Env) -> (RaffleFactoryClient<'_>, Address, Address) {
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let wasm_hash = BytesN::from_array(env, &[0u8; 32]);

        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(env, &contract_id);
        env.mock_all_auths();
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);
        client.set_creation_delay(&0u64);

        (client, admin, treasury)
    }

    fn test_raffle_config(env: &Env, payment_token: &Address) -> RaffleConfig {
        RaffleConfig {
            description: String::from_str(env, "Test Raffle"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 10,
            max_tickets_per_tx: 10,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: 10_000,
            payment_token: payment_token.clone(),
            prize_amount: 10_000,
            prizes: SdkVec::from_array(env, [10_000u32]),
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(env, &[1u8; 32]),
            claim_lockup_seconds: 0,
            swap_deadline_seconds: 0,
        }
    }

    fn create_raffles_via_factory(
        env: &Env,
        client: &RaffleFactoryClient<'_>,
        admin: &Address,
        treasury: &Address,
        creator: &Address,
        count: u32,
    ) -> SdkVec<Address> {
        use raffle_instance::ContractClient as RaffleInstanceClient;

        let factory_address = client.address.clone();
        let token_admin = Address::generate(env);
        let payment_token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let protocol_fee_bp: u32 = env.as_contract(&factory_address, || {
            env.storage()
                .persistent()
                .get(&DataKey::ProtocolFeeBP)
                .unwrap_or(0)
        });

        let mut addrs = SdkVec::new(env);
        for _ in 0..count {
            let mut config = test_raffle_config(env, &payment_token);
            config.protocol_fee_bp = protocol_fee_bp;
            config.treasury_address = Some(treasury.clone());

            let raffle_address = env.register(raffle_instance::Contract, ());
            RaffleInstanceClient::new(env, &raffle_address).init(
                &factory_address,
                admin,
                creator,
                &config,
            );

            env.as_contract(&factory_address, || {
                let stable_id: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::NextRaffleId)
                    .unwrap_or(0u32);
                env.storage()
                    .persistent()
                    .set(&DataKey::RaffleById(stable_id), &raffle_address);
                env.storage()
                    .persistent()
                    .set(&DataKey::NextRaffleId, &(stable_id.saturating_add(1)));
                let live_count: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::RaffleCount)
                    .unwrap_or(0u32)
                    .saturating_add(1);
                env.storage()
                    .persistent()
                    .set(&DataKey::RaffleCount, &live_count);
            });

            addrs.push_back(raffle_address);
        }
        addrs
    }

    #[test]
    fn test_init_factory() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _treasury) = setup_factory(&env);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_record_volume_overflow() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let asset = Address::generate(&env);

        client.record_volume(&asset, &(i128::MAX - 1));
        assert_eq!(client.get_total_volume(&asset), i128::MAX - 1);
        assert!(client.try_record_volume(&asset, &2).is_err());
        assert_eq!(client.get_total_volume(&asset), i128::MAX - 1);
    }

    #[test]
    fn test_set_config_rejects_excessive_protocol_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, treasury) = setup_factory(&env);
        let excessive_fee = MAX_PROTOCOL_FEE_BP + 1;

        assert_eq!(
            client.try_set_config(&excessive_fee, &treasury),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_second_call() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);
        assert_eq!(
            client.try_init_factory(&admin, &wasm_hash, &0u32, &treasury),
            Err(Ok(ContractError::AlreadyInitialized))
        );
    }

    /// Strkey of the all-zero contract id (the "zero address").
    const ZERO_CONTRACT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";

    fn zero_address(env: &Env) -> Address {
        Address::from_string(&String::from_str(env, ZERO_CONTRACT))
    }

    #[test]
    fn test_init_factory_rejects_zero_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&zero_address(&env), &wasm_hash, &0u32, &treasury),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_zero_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&admin, &wasm_hash, &0u32, &zero_address(&env)),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_self_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&contract_id, &wasm_hash, &0u32, &treasury),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_self_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&admin, &wasm_hash, &0u32, &contract_id),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_transfer_factory_admin_rejects_zero_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        assert_eq!(
            client.try_transfer_factory_admin(&zero_address(&env)),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_transfer_factory_admin_rejects_self() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let self_address = client.address.clone();

        assert_eq!(
            client.try_transfer_factory_admin(&self_address),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_set_config_rejects_zero_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        assert_eq!(
            client.try_set_config(&0u32, &zero_address(&env)),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_set_config_rejects_self_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let self_address = client.address.clone();

        assert_eq!(
            client.try_set_config(&0u32, &self_address),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_upgrade_requires_admin_authorization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);
        env.mock_all_auths();
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);

        let new_hash = BytesN::from_array(&env, &[9u8; 32]);
        // Without auth for the admin address, upgrade must not succeed.
        env.set_auths(&[]);
        assert!(client.try_upgrade(&new_hash).is_err());
    }

    // -----------------------------------------------------------------------
    // Stable-index storage tests (new with #426)
    //
    // These tests exercise the new storage layout directly via `env.as_contract`
    // to avoid the Soroban limitation that `env.register_at` cannot be called
    // from within an active contract invocation (which the test shim in
    // `create_raffle` does).  This approach tests the storage semantics cleanly.
    // -----------------------------------------------------------------------

    /// Seed the factory's stable-map storage with `n` synthetic raffle entries.
    fn seed_raffles(env: &Env, factory_id: &Address, n: u32) -> Vec<Address> {
        let mut addrs = Vec::new(env);
        env.as_contract(factory_id, || {
            for i in 0..n {
                let addr = Address::generate(env);
                env.storage()
                    .persistent()
                    .set(&DataKey::RaffleById(i), &addr);
                addrs.push_back(addr);
            }
            env.storage().persistent().set(&DataKey::NextRaffleId, &n);
            env.storage().persistent().set(&DataKey::RaffleCount, &n);
        });
        addrs
    }

    #[test]
    fn test_stable_ids_initial_state() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        // Before any raffle: NextRaffleId == 0, RaffleCount == 0.
        assert_eq!(client.get_next_raffle_id(), 0u32);
        assert_eq!(client.get_raffle_count(), 0u32);
        assert_eq!(client.get_raffle_by_id(&0u32), None);
    }

    #[test]
    fn test_stable_ids_seeded_lookup() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let addrs = seed_raffles(&env, &client.address, 3);

        assert_eq!(client.get_next_raffle_id(), 3u32);
        assert_eq!(client.get_raffle_count(), 3u32);
        assert_eq!(client.get_raffle_by_id(&0u32), Some(addrs.get(0).unwrap()));
        assert_eq!(client.get_raffle_by_id(&1u32), Some(addrs.get(1).unwrap()));
        assert_eq!(client.get_raffle_by_id(&2u32), Some(addrs.get(2).unwrap()));
        // Non-existent ID returns None.
        assert_eq!(client.get_raffle_by_id(&99u32), None);
    }

    #[test]
    fn test_get_raffles_page_returns_correct_slice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let addrs = seed_raffles(&env, &client.address, 5);

        // Page 0: offset=0, limit=3 → IDs 0,1,2.
        let page = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 3,
            offset: 0,
        });
        assert_eq!(page.items.len(), 3u32);
        assert_eq!(page.items.get(0).unwrap(), addrs.get(0).unwrap());
        assert_eq!(page.items.get(2).unwrap(), addrs.get(2).unwrap());
        assert!(page.has_more);

        // Page 1: offset=3, limit=3 → IDs 3,4 (only 2 remain).
        let page2 = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 3,
            offset: 3,
        });
        assert_eq!(page2.items.len(), 2u32);
        assert_eq!(page2.items.get(0).unwrap(), addrs.get(3).unwrap());
        assert_eq!(page2.items.get(1).unwrap(), addrs.get(4).unwrap());
        assert!(!page2.has_more);

        // Out-of-range offset → empty.
        let page3 = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 10,
            offset: 99,
        });
        assert_eq!(page3.items.len(), 0u32);
        assert!(!page3.has_more);
    }

    #[test]
    fn test_get_raffles_page_skips_tombstoned_slots() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let addrs = seed_raffles(&env, &client.address, 3);

        // Tombstone slot 1 directly in storage.
        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .remove(&DataKey::RaffleById(1u32));
            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::RaffleCount)
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::RaffleCount, &count.saturating_sub(1));
        });

        assert_eq!(client.get_raffle_count(), 2u32);
        assert_eq!(client.get_next_raffle_id(), 3u32); // monotonic, unchanged
        assert_eq!(client.get_raffle_by_id(&1u32), None);

        // Page over all IDs; tombstoned slot 1 is skipped.
        let page = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 10,
            offset: 0,
        });
        assert_eq!(page.items.len(), 2u32);
        assert_eq!(page.items.get(0).unwrap(), addrs.get(0).unwrap());
        assert_eq!(page.items.get(1).unwrap(), addrs.get(2).unwrap());
    }

    #[test]
    fn get_raffles_page_empty_list() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 0,
        });
        assert_eq!(page.items.len(), 0u32);
        assert_eq!(page.total, 0u32);
        assert!(!page.has_more);
    }

    #[test]
    fn get_raffles_page_first_page() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 15);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 0,
        });
        assert_eq!(page.items.len(), 10u32);
        assert_eq!(page.total, 15u32);
        assert!(page.has_more);
    }

    #[test]
    fn get_raffles_page_last_page() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 15);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 10,
        });
        assert_eq!(page.items.len(), 5u32);
        assert_eq!(page.total, 15u32);
        assert!(!page.has_more);
    }

    #[test]
    fn get_raffles_page_offset_beyond_total() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 5);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 10,
        });
        assert_eq!(page.items.len(), 0u32);
        assert_eq!(page.total, 5u32);
        assert!(!page.has_more);
    }

    #[test]
    fn get_raffles_page_limit_zero_uses_default() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 150);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 0,
            offset: 0,
        });
        assert_eq!(page.items.len(), DEFAULT_PAGE_LIMIT);
        assert_eq!(page.total, 150u32);
        assert!(page.has_more);
    }

    #[test]
    fn get_raffles_page_limit_above_max_is_clamped() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 250);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 999,
            offset: 0,
        });
        assert_eq!(page.items.len(), MAX_PAGE_LIMIT);
        assert_eq!(page.total, 250u32);
        assert!(page.has_more);
    }

    #[test]
    fn test_clean_old_raffle_invalid_id_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        // No raffles → any ID is invalid.
        assert_eq!(
            client.try_clean_old_raffle(&0u32),
            Err(Ok(ContractError::InvalidRaffleId))
        );
    }

    #[test]
    fn test_clean_old_raffle_already_tombstoned_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        seed_raffles(&env, &client.address, 3);

        // Tombstone slot 1.
        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .remove(&DataKey::RaffleById(1u32));
        });

        // Trying to clean it again must return InvalidRaffleId.
        assert_eq!(
            client.try_clean_old_raffle(&1u32),
            Err(Ok(ContractError::InvalidRaffleId))
        );
    }

    // -----------------------------------------------------------------------
    // Creator index tests
    // -----------------------------------------------------------------------

    /// Seed the per-creator index directly in storage with `addrs`.
    fn seed_creator_index(env: &Env, factory_id: &Address, creator: &Address, addrs: &[Address]) {
        env.as_contract(factory_id, || {
            let mut v: Vec<Address> = Vec::new(env);
            for a in addrs {
                v.push_back(a.clone());
            }
            env.storage()
                .persistent()
                .set(&DataKey::CreatorRaffles(creator.clone()), &v);
        });
    }

    #[test]
    fn test_get_raffles_by_creator_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);

        let page = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(page.items.len(), 0u32);
        assert_eq!(page.total, 0u32);
        assert!(!page.has_more);
    }

    #[test]
    fn test_get_raffles_by_creator_basic() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let creator_a = Address::generate(&env);
        let creator_b = Address::generate(&env);

        // 5 raffles for A, 3 for B.
        let mut a_addrs = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        let b_addrs = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];

        seed_creator_index(&env, &client.address, &creator_a, &a_addrs);
        seed_creator_index(&env, &client.address, &creator_b, &b_addrs);

        // Creator A: full page.
        let page_a = client.get_raffles_by_creator(
            &creator_a,
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(page_a.total, 5u32);
        assert_eq!(page_a.items.len(), 5u32);
        assert!(!page_a.has_more);
        for (i, addr) in a_addrs.iter().enumerate() {
            assert_eq!(page_a.items.get(i as u32).unwrap(), addr.clone());
        }

        // Creator B: full page.
        let page_b = client.get_raffles_by_creator(
            &creator_b,
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(page_b.total, 3u32);
        assert_eq!(page_b.items.len(), 3u32);
        assert!(!page_b.has_more);
        for (i, addr) in b_addrs.iter().enumerate() {
            assert_eq!(page_b.items.get(i as u32).unwrap(), addr.clone());
        }
    }

    #[test]
    fn test_get_raffles_by_creator_pagination() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let creator = Address::generate(&env);
        let addrs = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        seed_creator_index(&env, &client.address, &creator, &addrs);

        // Page 0: offset=0, limit=3 → items 0,1,2; has_more=true.
        let p0 = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams {
                limit: 3,
                offset: 0,
            },
        );
        assert_eq!(p0.items.len(), 3u32);
        assert_eq!(p0.total, 5u32);
        assert!(p0.has_more);
        assert_eq!(p0.items.get(0).unwrap(), addrs[0].clone());
        assert_eq!(p0.items.get(2).unwrap(), addrs[2].clone());

        // Page 1: offset=3, limit=3 → items 3,4; has_more=false.
        let p1 = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams {
                limit: 3,
                offset: 3,
            },
        );
        assert_eq!(p1.items.len(), 2u32);
        assert_eq!(p1.total, 5u32);
        assert!(!p1.has_more);
        assert_eq!(p1.items.get(0).unwrap(), addrs[3].clone());
        assert_eq!(p1.items.get(1).unwrap(), addrs[4].clone());

        // Out-of-range offset → empty, has_more=false.
        let p_oor = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 99,
            },
        );
        assert_eq!(p_oor.items.len(), 0u32);
        assert!(!p_oor.has_more);

        // Exact boundary: offset=5 (== total) → empty.
        let p_exact = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 5,
            },
        );
        assert_eq!(p_exact.items.len(), 0u32);
        assert!(!p_exact.has_more);
    }

    #[test]
    fn test_creator_index_isolates_separate_creators() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let creator_a = Address::generate(&env);
        let creator_b = Address::generate(&env);

        let a_addrs = [Address::generate(&env), Address::generate(&env)];
        let b_addrs = [Address::generate(&env)];

        seed_creator_index(&env, &client.address, &creator_a, &a_addrs);
        seed_creator_index(&env, &client.address, &creator_b, &b_addrs);

        // A sees only its own raffles.
        let pa = client.get_raffles_by_creator(
            &creator_a,
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(pa.total, 2u32);

        // B sees only its own raffle.
        let pb = client.get_raffles_by_creator(
            &creator_b,
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(pb.total, 1u32);
        assert_eq!(pb.items.get(0).unwrap(), b_addrs[0].clone());
    }

    // -----------------------------------------------------------------------
    // Factory admin two-step transfer tests (#453)
    // -----------------------------------------------------------------------

    #[test]
    fn test_admin_transfer_two_step_completes_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let new_admin = Address::generate(&env);

        client.transfer_factory_admin(&new_admin);
        client.accept_factory_admin();

        let actual: Address = env.as_contract(&client.address, || {
            env.storage().persistent().get(&DataKey::Admin).unwrap()
        });
        assert_eq!(actual, new_admin);

        let pending_still_exists: bool = env.as_contract(&client.address, || {
            env.storage().persistent().has(&DataKey::PendingAdmin)
        });
        assert!(!pending_still_exists);
    }

    #[test]
    fn test_admin_transfer_rejected_if_pending_already_exists() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let admin_b = Address::generate(&env);
        let admin_c = Address::generate(&env);

        client.transfer_factory_admin(&admin_b);

        assert_eq!(
            client.try_transfer_factory_admin(&admin_c),
            Err(Ok(ContractError::AdminTransferPending))
        );
    }

    #[test]
    fn test_admin_accept_fails_if_wrong_address_accepts() {
        let env = Env::default();
        let (client, _admin, _treasury) = setup_factory(&env);
        let admin_b = Address::generate(&env);
        let admin_c = Address::generate(&env);

        env.mock_all_auths();
        client.transfer_factory_admin(&admin_b);

        // Only admin_c is authorized — admin_b is the pending, so rejecting admin_b
        // proves the caller must match PendingAdmin.
        env.mock_auths(&[&admin_b]);
        assert!(client.try_accept_factory_admin().is_err());
    }

    #[test]
    fn test_admin_transfer_to_same_address_clears_pending() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _treasury) = setup_factory(&env);
        let new_admin = Address::generate(&env);

        client.transfer_factory_admin(&new_admin);

        let pending_before: bool = env.as_contract(&client.address, || {
            env.storage().persistent().has(&DataKey::PendingAdmin)
        });
        assert!(pending_before);

        // Proposing the current admin clears the pending entry
        client.transfer_factory_admin(&admin);

        let pending_after: bool = env.as_contract(&client.address, || {
            env.storage().persistent().has(&DataKey::PendingAdmin)
        });
        assert!(!pending_after);

        let actual: Address = env.as_contract(&client.address, || {
            env.storage().persistent().get(&DataKey::Admin).unwrap()
        });
        assert_eq!(actual, admin);
    }

    #[test]
    fn test_only_new_admin_can_accept_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _treasury) = setup_factory(&env);
        let new_admin = Address::generate(&env);

        client.transfer_factory_admin(&new_admin);

        // Old admin tries to accept — should fail because require_auth checks caller == PendingAdmin
        env.mock_auths(&[&admin]);
        assert!(client.try_accept_factory_admin().is_err());
    }

    // -----------------------------------------------------------------------
    // Rate limiter tests (#447)
    //
    // The rate limiter lives inside `create_raffle` and gates non-whitelisted
    // creators to at most one creation per `MinCreationDelay` seconds.  These
    // tests exercise the full `create_raffle` path (deploying a real instance
    // via the test shim) so the guard is validated end-to-end.
    // -----------------------------------------------------------------------

    /// A complete, valid `RaffleConfig` for rate-limiter tests.  Prize tiers sum
    /// to 10_000 bp and `prize_amount >= ticket_price`, satisfying instance init.
    fn rate_limit_config(env: &Env, payment_token: &Address, desc: &str) -> RaffleConfig {
        RaffleConfig {
            description: String::from_str(env, desc),
            end_time: 0,
            no_deadline: true,
            max_tickets: 10,
            max_tickets_per_tx: 10,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: 10_000,
            payment_token: payment_token.clone(),
            prize_amount: 10_000,
            prizes: SdkVec::from_array(env, [10_000u32]),
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(env, &[1u8; 32]),
            claim_lockup_seconds: 0,
            swap_deadline_seconds: 0,
            early_bird_ticket_percentage: 0,
            early_bird_discount_bp: 0,
            category: None,
        }
    }

    /// Register a payment token the instance init will accept.
    fn make_token(env: &Env) -> Address {
        let token_admin = Address::generate(env);
        env.register_stellar_asset_contract_v2(token_admin)
            .address()
    }

    #[test]
    fn non_whitelisted_creator_is_rate_limited() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let (client, _admin, _treasury) = setup_factory(&env);
        // Use a real, non-zero delay (setup_factory zeroes it by default).
        let delay: u64 = 300;
        client.set_creation_delay(&delay);

        let creator = Address::generate(&env);
        let token = make_token(&env);

        // 1. First creation succeeds.
        client.create_raffle(&creator, &rate_limit_config(&env, &token, "r1"));

        // 2. Immediate second creation is rate-limited.
        assert_eq!(
            client.try_create_raffle(&creator, &rate_limit_config(&env, &token, "r2")),
            Err(Ok(ContractError::RateLimitExceeded))
        );

        // 3. Advance time by exactly MinCreationDelay.
        env.ledger().set_timestamp(1_000 + delay);

        // 4. Creation succeeds again once the window has elapsed.
        client.create_raffle(&creator, &rate_limit_config(&env, &token, "r3"));
    }

    #[test]
    fn whitelisted_partner_bypasses_rate_limit() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let (client, _admin, _treasury) = setup_factory(&env);
        client.set_creation_delay(&300u64);

        let creator = Address::generate(&env);
        let token = make_token(&env);

        // Whitelist the creator, then create twice back-to-back with no time
        // advance — both must succeed because the whitelist bypasses the limiter.
        client.set_whitelist_status(&creator, &true);
        client.create_raffle(&creator, &rate_limit_config(&env, &token, "w1"));
        client.create_raffle(&creator, &rate_limit_config(&env, &token, "w2"));
    }

    #[test]
    fn set_creation_delay_affects_rate_limiter() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let (client, _admin, _treasury) = setup_factory(&env);
        client.set_creation_delay(&60u64);

        let creator = Address::generate(&env);
        let token = make_token(&env);

        // 1. Create at t=1000.
        client.create_raffle(&creator, &rate_limit_config(&env, &token, "d1"));

        // 2. Advance 59 seconds — still inside the window, second creation fails.
        env.ledger().set_timestamp(1_000 + 59);
        assert_eq!(
            client.try_create_raffle(&creator, &rate_limit_config(&env, &token, "d2")),
            Err(Ok(ContractError::RateLimitExceeded))
        );

        // 3. Advance 1 more second (60 total) — the window has elapsed, succeeds.
        env.ledger().set_timestamp(1_000 + 60);
        client.create_raffle(&creator, &rate_limit_config(&env, &token, "d3"));
    }

    // -----------------------------------------------------------------------
    // Category index tests (#439)
    // -----------------------------------------------------------------------

    /// Seed the per-category index directly in storage (mirrors
    /// `seed_creator_index`) so `get_raffles_by_category` can be validated
    /// without going through the `create_raffle` deploy shim.
    fn seed_category_index(env: &Env, factory_id: &Address, category: &str, addrs: &[Address]) {
        let cat = String::from_str(env, category);
        env.as_contract(factory_id, || {
            let mut v: Vec<Address> = Vec::new(env);
            for a in addrs {
                v.push_back(a.clone());
            }
            env.storage()
                .persistent()
                .set(&DataKey::CategoryRaffles(cat.clone()), &v);
        });
    }

    #[test]
    fn get_raffles_by_category_unknown_is_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let page = client.get_raffles_by_category(
            &String::from_str(&env, "gaming"),
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(page.items.len(), 0u32);
        assert_eq!(page.total, 0u32);
        assert!(!page.has_more);
    }

    #[test]
    fn get_raffles_by_category_returns_only_matching() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        // 3 raffles tagged "gaming", 2 tagged "art".
        let gaming = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        let art = [Address::generate(&env), Address::generate(&env)];

        seed_category_index(&env, &client.address, "gaming", &gaming);
        seed_category_index(&env, &client.address, "art", &art);

        let gaming_page = client.get_raffles_by_category(
            &String::from_str(&env, "gaming"),
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(gaming_page.total, 3u32);
        assert_eq!(gaming_page.items.len(), 3u32);
        assert!(!gaming_page.has_more);
        for (i, addr) in gaming.iter().enumerate() {
            assert_eq!(gaming_page.items.get(i as u32).unwrap(), addr.clone());
        }

        let art_page = client.get_raffles_by_category(
            &String::from_str(&env, "art"),
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(art_page.total, 2u32);
        assert_eq!(art_page.items.len(), 2u32);
        assert!(!art_page.has_more);

        // A category with no raffles yields an empty page.
        let charity_page = client.get_raffles_by_category(
            &String::from_str(&env, "charity"),
            &raffle_shared::PaginationParams {
                limit: 10,
                offset: 0,
            },
        );
        assert_eq!(charity_page.total, 0u32);
        assert_eq!(charity_page.items.len(), 0u32);
    }

    #[test]
    fn get_raffles_by_category_paginates() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let addrs = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        seed_category_index(&env, &client.address, "gaming", &addrs);

        // Page 0: offset=0, limit=3 → items 0,1,2; has_more=true.
        let p0 = client.get_raffles_by_category(
            &String::from_str(&env, "gaming"),
            &raffle_shared::PaginationParams {
                limit: 3,
                offset: 0,
            },
        );
        assert_eq!(p0.items.len(), 3u32);
        assert_eq!(p0.total, 5u32);
        assert!(p0.has_more);
        assert_eq!(p0.items.get(0).unwrap(), addrs[0].clone());
        assert_eq!(p0.items.get(2).unwrap(), addrs[2].clone());

        // Page 1: offset=3, limit=3 → items 3,4; has_more=false.
        let p1 = client.get_raffles_by_category(
            &String::from_str(&env, "gaming"),
            &raffle_shared::PaginationParams {
                limit: 3,
                offset: 3,
            },
        );
        assert_eq!(p1.items.len(), 2u32);
        assert!(!p1.has_more);
        assert_eq!(p1.items.get(0).unwrap(), addrs[3].clone());
        assert_eq!(p1.items.get(1).unwrap(), addrs[4].clone());
    }
}
