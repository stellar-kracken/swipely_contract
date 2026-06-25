/**
 * Emergency Fund Recovery Module
 * Allows authorized parties to recover funds from the contract in emergency scenarios.
 * Includes recovery authorization, fund destination management, time locks, and event emission.
 */

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
    token::Client as TokenClient,
};

const EVENT_RECOVERY_INITIATED: Symbol = symbol_short!("rec_init");
const EVENT_RECOVERY_APPROVED: Symbol = symbol_short!("rec_app");
const EVENT_RECOVERY_EXECUTED: Symbol = symbol_short!("rec_exec");
const EVENT_RECOVERY_CANCELLED: Symbol = symbol_short!("rec_can");
const EVENT_RECOVERY_HISTORY: Symbol = symbol_short!("rec_his");

const DEFAULT_RECOVERY_TIMELOCK_SECONDS: u64 = 172_800; // 48 hours

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RecoveryError {
    NotAuthorized = 1,
    InvalidAmount = 2,
    InvalidDestination = 3,
    RecoveryNotFound = 4,
    AlreadyApproved = 5,
    InsufficientApprovals = 6,
    TimelockNotElapsed = 7,
    RecoveryAlreadyExecuted = 8,
    RecoveryAlreadyCancelled = 9,
    InvalidRecoveryState = 10,
    EmergencyModeDisabled = 11,
    NoFundsToRecover = 12,
    TokenTransferFailed = 13,
    InvalidTimelock = 14,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryStatus {
    Pending,
    Approved,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyRecovery {
    pub recovery_id: u64,
    pub initiator: Address,
    pub destination: Address,
    pub token_address: Address,
    pub amount: i128,
    pub reason: String,
    pub initiated_at: u64,
    pub timelock_until: u64,
    pub status: RecoveryStatus,
    pub approvals: Vec<Address>,
    pub required_approvals: u32,
    pub executed_at: u64,
    pub executed_by: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryAuthorization {
    pub address: Address,
    pub can_initiate: bool,
    pub can_approve: bool,
    pub can_execute: bool,
    pub can_cancel: bool,
    pub added_at: u64,
}

#[contracttype]
pub enum RecoveryDataKey {
    Admin,
    EmergencyEnabled,
    RecoverySeq,
    Recovery(u64),
    RecoveryHistory,
    AuthorizedRecoverers,
    Timelock,
    TotalRecovered,
}

/// Emergency Fund Recovery Service
pub struct EmergencyFundRecovery;

#[contractimpl]
impl EmergencyFundRecovery {
    /// Initialize emergency fund recovery with admin and timelock settings
    pub fn initialize_recovery(
        env: Env,
        admin: Address,
        timelock_seconds: u64,
    ) -> Result<(), RecoveryError> {
        admin.require_auth();

        if timelock_seconds == 0 || timelock_seconds > 31_536_000 {
            // Max 1 year
            return Err(RecoveryError::InvalidTimelock);
        }

        env.storage()
            .instance()
            .set(&RecoveryDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&RecoveryDataKey::EmergencyEnabled, &false);
        env.storage()
            .instance()
            .set(&RecoveryDataKey::RecoverySeq, &0u64);
        env.storage()
            .instance()
            .set(&RecoveryDataKey::Timelock, &timelock_seconds);
        env.storage()
            .instance()
            .set(&RecoveryDataKey::TotalRecovered, &0i128);
        env.storage()
            .instance()
            .set(&RecoveryDataKey::AuthorizedRecoverers, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .set(&RecoveryDataKey::RecoveryHistory, &Vec::<u64>::new(&env));

        Ok(())
    }

    /// Enable emergency fund recovery mode (requires admin authorization)
    pub fn enable_emergency_recovery(env: Env, admin: Address) -> Result<(), RecoveryError> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::Admin)
            .ok_or(RecoveryError::NotAuthorized)?;

        if admin != stored_admin {
            return Err(RecoveryError::NotAuthorized);
        }

        env.storage()
            .instance()
            .set(&RecoveryDataKey::EmergencyEnabled, &true);

        env.events().publish(
            ("emergency_recovery", "enabled"),
            ("timestamp", env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Disable emergency fund recovery mode
    pub fn disable_emergency_recovery(env: Env, admin: Address) -> Result<(), RecoveryError> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::Admin)
            .ok_or(RecoveryError::NotAuthorized)?;

        if admin != stored_admin {
            return Err(RecoveryError::NotAuthorized);
        }

        env.storage()
            .instance()
            .set(&RecoveryDataKey::EmergencyEnabled, &false);

        env.events().publish(
            ("emergency_recovery", "disabled"),
            ("timestamp", env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Add an authorized recovery user with specific permissions
    pub fn add_recovery_authorizer(
        env: Env,
        admin: Address,
        user: Address,
        can_initiate: bool,
        can_approve: bool,
        can_execute: bool,
        can_cancel: bool,
    ) -> Result<(), RecoveryError> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::Admin)
            .ok_or(RecoveryError::NotAuthorized)?;

        if admin != stored_admin {
            return Err(RecoveryError::NotAuthorized);
        }

        let authorization = RecoveryAuthorization {
            address: user.clone(),
            can_initiate,
            can_approve,
            can_execute,
            can_cancel,
            added_at: env.ledger().timestamp(),
        };

        let key = (&user,);
        env.storage()
            .persistent()
            .set(&key, &authorization);

        Ok(())
    }

    /// Initiate an emergency fund recovery
    pub fn initiate_recovery(
        env: Env,
        initiator: Address,
        destination: Address,
        token_address: Address,
        amount: i128,
        reason: String,
    ) -> Result<u64, RecoveryError> {
        initiator.require_auth();

        // Check if emergency recovery is enabled
        let enabled: bool = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::EmergencyEnabled)
            .ok_or(RecoveryError::EmergencyModeDisabled)?;

        if !enabled {
            return Err(RecoveryError::EmergencyModeDisabled);
        }

        // Check authorization
        let auth_key = (&initiator,);
        let auth: Option<RecoveryAuthorization> = env.storage().persistent().get(&auth_key);

        if let Some(auth) = auth {
            if !auth.can_initiate {
                return Err(RecoveryError::NotAuthorized);
            }
        } else {
            return Err(RecoveryError::NotAuthorized);
        }

        // Validate inputs
        if amount <= 0 {
            return Err(RecoveryError::InvalidAmount);
        }

        if destination == initiator {
            return Err(RecoveryError::InvalidDestination);
        }

        // Get recovery sequence and increment
        let recovery_seq: u64 = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::RecoverySeq)
            .unwrap_or(0);
        let recovery_id = recovery_seq + 1;

        env.storage()
            .instance()
            .set(&RecoveryDataKey::RecoverySeq, &recovery_id);

        // Get timelock
        let timelock: u64 = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::Timelock)
            .unwrap_or(DEFAULT_RECOVERY_TIMELOCK_SECONDS);

        let current_time = env.ledger().timestamp();
        let timelock_until = current_time + timelock;

        // Create recovery record
        let recovery = EmergencyRecovery {
            recovery_id,
            initiator: initiator.clone(),
            destination: destination.clone(),
            token_address: token_address.clone(),
            amount,
            reason: reason.clone(),
            initiated_at: current_time,
            timelock_until,
            status: RecoveryStatus::Pending,
            approvals: Vec::new(&env),
            required_approvals: 1, // Will be set based on approvers
            executed_at: 0,
            executed_by: Address::generate(&env),
        };

        // Store recovery
        let key = (recovery_id,);
        env.storage().persistent().set(&key, &recovery);

        // Add to history
        let mut history: Vec<u64> = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::RecoveryHistory)
            .unwrap_or_else(|| Vec::new(&env));
        history.push_back(recovery_id);
        env.storage()
            .instance()
            .set(&RecoveryDataKey::RecoveryHistory, &history);

        // Emit event
        env.events().publish(
            ("emergency_recovery", "initiated"),
            (
                ("recovery_id", recovery_id),
                ("initiator", initiator),
                ("destination", destination),
                ("amount", amount),
                ("timelock_until", timelock_until),
            ),
        );

        Ok(recovery_id)
    }

    /// Approve an emergency fund recovery
    pub fn approve_recovery(
        env: Env,
        approver: Address,
        recovery_id: u64,
    ) -> Result<(), RecoveryError> {
        approver.require_auth();

        // Get recovery
        let key = (recovery_id,);
        let mut recovery: EmergencyRecovery = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RecoveryError::RecoveryNotFound)?;

        // Check if already approved
        if recovery.status != RecoveryStatus::Pending {
            return Err(RecoveryError::InvalidRecoveryState);
        }

        // Check authorization
        let auth_key = (&approver,);
        let auth: Option<RecoveryAuthorization> = env.storage().persistent().get(&auth_key);

        if let Some(auth) = auth {
            if !auth.can_approve {
                return Err(RecoveryError::NotAuthorized);
            }
        } else {
            return Err(RecoveryError::NotAuthorized);
        }

        // Check if already approved by this address
        for approved_addr in recovery.approvals.iter() {
            if approved_addr == approver {
                return Err(RecoveryError::AlreadyApproved);
            }
        }

        // Add approval
        recovery.approvals.push_back(approver.clone());

        // Check if we have enough approvals
        if recovery.approvals.len() >= recovery.required_approvals as usize {
            recovery.status = RecoveryStatus::Approved;
        }

        // Update recovery
        env.storage()
            .persistent()
            .set(&key, &recovery);

        // Emit event
        env.events().publish(
            ("emergency_recovery", "approved"),
            (
                ("recovery_id", recovery_id),
                ("approver", approver),
                ("approvals_count", recovery.approvals.len()),
            ),
        );

        Ok(())
    }

    /// Execute an approved emergency fund recovery
    pub fn execute_recovery(
        env: Env,
        executor: Address,
        recovery_id: u64,
    ) -> Result<(), RecoveryError> {
        executor.require_auth();

        // Get recovery
        let key = (recovery_id,);
        let mut recovery: EmergencyRecovery = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RecoveryError::RecoveryNotFound)?;

        // Check status
        if recovery.status == RecoveryStatus::Executed {
            return Err(RecoveryError::RecoveryAlreadyExecuted);
        }

        if recovery.status == RecoveryStatus::Cancelled {
            return Err(RecoveryError::RecoveryAlreadyCancelled);
        }

        if recovery.status != RecoveryStatus::Approved {
            return Err(RecoveryError::InvalidRecoveryState);
        }

        // Check timelock
        let current_time = env.ledger().timestamp();
        if current_time < recovery.timelock_until {
            return Err(RecoveryError::TimelockNotElapsed);
        }

        // Check authorization
        let auth_key = (&executor,);
        let auth: Option<RecoveryAuthorization> = env.storage().persistent().get(&auth_key);

        if let Some(auth) = auth {
            if !auth.can_execute {
                return Err(RecoveryError::NotAuthorized);
            }
        } else {
            return Err(RecoveryError::NotAuthorized);
        }

        // Transfer funds via token contract
        let token_client = TokenClient::new(&env, &recovery.token_address);
        token_client.transfer(&env.current_contract_address(), &recovery.destination, &recovery.amount);

        // Update recovery status
        recovery.status = RecoveryStatus::Executed;
        recovery.executed_at = current_time;
        recovery.executed_by = executor.clone();

        env.storage()
            .persistent()
            .set(&key, &recovery);

        // Update total recovered
        let mut total: i128 = env
            .storage()
            .instance()
            .get(&RecoveryDataKey::TotalRecovered)
            .unwrap_or(0);
        total += recovery.amount;
        env.storage()
            .instance()
            .set(&RecoveryDataKey::TotalRecovered, &total);

        // Emit event
        env.events().publish(
            ("emergency_recovery", "executed"),
            (
                ("recovery_id", recovery_id),
                ("executor", executor),
                ("destination", recovery.destination),
                ("amount", recovery.amount),
            ),
        );

        Ok(())
    }

    /// Cancel a pending recovery
    pub fn cancel_recovery(
        env: Env,
        canceller: Address,
        recovery_id: u64,
        reason: String,
    ) -> Result<(), RecoveryError> {
        canceller.require_auth();

        // Get recovery
        let key = (recovery_id,);
        let mut recovery: EmergencyRecovery = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RecoveryError::RecoveryNotFound)?;

        // Check status
        if recovery.status == RecoveryStatus::Executed {
            return Err(RecoveryError::RecoveryAlreadyExecuted);
        }

        if recovery.status == RecoveryStatus::Cancelled {
            return Err(RecoveryError::RecoveryAlreadyCancelled);
        }

        // Check authorization
        let auth_key = (&canceller,);
        let auth: Option<RecoveryAuthorization> = env.storage().persistent().get(&auth_key);

        if let Some(auth) = auth {
            if !auth.can_cancel {
                return Err(RecoveryError::NotAuthorized);
            }
        } else {
            return Err(RecoveryError::NotAuthorized);
        }

        // Cancel recovery
        recovery.status = RecoveryStatus::Cancelled;

        env.storage()
            .persistent()
            .set(&key, &recovery);

        // Emit event
        env.events().publish(
            ("emergency_recovery", "cancelled"),
            (
                ("recovery_id", recovery_id),
                ("cancelled_by", canceller),
                ("reason", reason),
            ),
        );

        Ok(())
    }

    /// Get recovery details
    pub fn get_recovery(env: Env, recovery_id: u64) -> Result<EmergencyRecovery, RecoveryError> {
        let key = (recovery_id,);
        env.storage()
            .persistent()
            .get(&key)
            .ok_or(RecoveryError::RecoveryNotFound)
    }

    /// Get total funds recovered
    pub fn get_total_recovered(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&RecoveryDataKey::TotalRecovered)
            .unwrap_or(0)
    }

    /// Get recovery history
    pub fn get_recovery_history(env: Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&RecoveryDataKey::RecoveryHistory)
            .unwrap_or_else(|| Vec::new(&env))
    }
}
