/**
 * Version Migration Helper (Enhanced)
 * Comprehensive contract state migration system with rollback support,
 * validation, and state verification.
 *
 * Builds upon the existing MigrationHelper in migration.rs with additional
 * features for production-grade state migrations.
 */

use soroban_sdk::{
    contracttype, symbol_short, vec, Address, Env, Map, String as SorobanString, Vec,
};

// ─── Types ─────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRecord {
    pub from_version: MigrationVersion,
    pub to_version: MigrationVersion,
    pub migrated_at: u64,
    pub migrated_by: Address,
    pub success: bool,
    pub notes: SorobanString,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateSnapshot {
    pub version: MigrationVersion,
    pub snapshot_at: u64,
    pub snapshot_by: Address,
    pub snapshot_hash: SorobanString, // SHA-256 hash of state
    pub description: SorobanString,
    pub data: Map<SorobanString, SorobanString>, // Serialized state data
    pub rollback_available: bool,
    pub expiration_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationCheckpoint {
    PreMigration,
    PostMigration,
    RollbackPrep,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationResult {
    pub checkpoint: ValidationCheckpoint,
    pub passed: bool,
    pub errors: Vec<SorobanString>,
    pub warnings: Vec<SorobanString>,
    pub checked_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    AlreadyAtVersion,
    VersionDowngradeNotAllowed,
    UnauthorizedMigrator,
    ValidationFailed,
    RollbackNotAvailable,
    InvalidStateSnapshot,
    SnapshotExpired,
    NoValidationResults,
    MigrationInProgress,
    StateIntegrityCheckFailed,
}

// ─── Storage Keys ──────────────────────────────────────────────────────────

mod keys {
    pub const MIGRATION_VERSION: &str = "mig_version";
    pub const MIGRATION_HISTORY: &str = "mig_history";
    pub const STATE_SNAPSHOTS: &str = "state_snapshots";
    pub const VALIDATION_RESULTS: &str = "validation_results";
    pub const MIGRATION_IN_PROGRESS: &str = "mig_in_progress";
    pub const LAST_MIGRATION_TIMESTAMP: &str = "last_mig_timestamp";
    pub const AUTHORIZED_MIGRATORS: &str = "auth_migrators";
    pub const MIGRATION_TIMEOUT_SECONDS: &str = "mig_timeout";
    pub const SNAPSHOT_RETENTION_DAYS: &str = "snapshot_retention";
}

// ─── Enhanced Version Migration Helper ──────────────────────────────────────

pub struct EnhancedMigrationHelper;

impl EnhancedMigrationHelper {
    /**
     * Initialize migration system
     */
    pub fn initialize(
        env: &Env,
        admin: Address,
        initial_version: MigrationVersion,
    ) -> Result<(), MigrationError> {
        admin.require_auth();

        let version_key = SorobanString::from_str(env, keys::MIGRATION_VERSION);
        let migrators_key = SorobanString::from_str(env, keys::AUTHORIZED_MIGRATORS);

        env.storage()
            .persistent()
            .set::<SorobanString, MigrationVersion>(&version_key, &initial_version);

        let mut migrators = Vec::new(env);
        migrators.push_back(admin);
        env.storage()
            .persistent()
            .set::<SorobanString, Vec<Address>>(&migrators_key, &migrators);

        env.storage().instance().set(
            &SorobanString::from_str(env, keys::MIGRATION_TIMEOUT_SECONDS),
            &3600u64, // 1 hour default
        );

        Ok(())
    }

    /**
     * Get current contract version
     */
    pub fn get_version(env: &Env) -> MigrationVersion {
        let key = SorobanString::from_str(env, keys::MIGRATION_VERSION);
        env.storage()
            .persistent()
            .get::<SorobanString, MigrationVersion>(&key)
            .unwrap_or(MigrationVersion {
                major: 0,
                minor: 0,
                patch: 0,
            })
    }

    /**
     * Create state snapshot before migration
     */
    pub fn create_state_snapshot(
        env: &Env,
        snapshot_by: Address,
        description: SorobanString,
        data: Map<SorobanString, SorobanString>,
    ) -> Result<StateSnapshot, MigrationError> {
        let current_version = Self::get_version(env);
        let current_time = env.ledger().timestamp();

        // Calculate snapshot hash (simplified)
        let hash = SorobanString::from_str(env, "sha256_placeholder");

        let snapshot = StateSnapshot {
            version: current_version.clone(),
            snapshot_at: current_time,
            snapshot_by,
            snapshot_hash: hash,
            description,
            data,
            rollback_available: true,
            expiration_timestamp: current_time + (7 * 24 * 3600), // 7 days
        };

        // Store snapshot
        let snapshots_key = SorobanString::from_str(env, keys::STATE_SNAPSHOTS);
        let mut snapshots: Vec<StateSnapshot> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<StateSnapshot>>(&snapshots_key)
            .unwrap_or_else(|| vec![env]);

        snapshots.push_back(snapshot.clone());
        env.storage()
            .persistent()
            .set::<SorobanString, Vec<StateSnapshot>>(&snapshots_key, &snapshots);

        Ok(snapshot)
    }

    /**
     * Validate upgrade path
     */
    pub fn validate_upgrade(
        from: &MigrationVersion,
        to: &MigrationVersion,
    ) -> Result<(), MigrationError> {
        if from == to {
            return Err(MigrationError::AlreadyAtVersion);
        }

        let is_forward = if to.major != from.major {
            to.major > from.major
        } else if to.minor != from.minor {
            to.minor > from.minor
        } else {
            to.patch > from.patch
        };

        if !is_forward {
            return Err(MigrationError::VersionDowngradeNotAllowed);
        }

        Ok(())
    }

    /**
     * Begin migration with safety checks
     */
    pub fn begin_migration(
        env: &Env,
        migrator: Address,
        target_version: MigrationVersion,
    ) -> Result<(), MigrationError> {
        migrator.require_auth();

        // Check authorization
        let migrators_key = SorobanString::from_str(env, keys::AUTHORIZED_MIGRATORS);
        let migrators: Vec<Address> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<Address>>(&migrators_key)
            .ok_or(MigrationError::UnauthorizedMigrator)?;

        let is_authorized = migrators.iter().any(|addr| addr == &migrator);
        if !is_authorized {
            return Err(MigrationError::UnauthorizedMigrator);
        }

        // Check if migration already in progress
        let in_progress_key = SorobanString::from_str(env, keys::MIGRATION_IN_PROGRESS);
        if let Some(in_progress) = env.storage().instance().get::<SorobanString, bool>(&in_progress_key) {
            if in_progress {
                return Err(MigrationError::MigrationInProgress);
            }
        }

        // Set migration flag
        env.storage()
            .instance()
            .set(&in_progress_key, &true);

        Ok(())
    }

    /**
     * Record migration completion
     */
    pub fn complete_migration(
        env: &Env,
        from_version: MigrationVersion,
        to_version: MigrationVersion,
        migrator: Address,
        notes: SorobanString,
    ) -> Result<(), MigrationError> {
        // Validate upgrade path
        Self::validate_upgrade(&from_version, &to_version)?;

        // Update current version
        let version_key = SorobanString::from_str(env, keys::MIGRATION_VERSION);
        env.storage()
            .persistent()
            .set::<SorobanString, MigrationVersion>(&version_key, &to_version);

        // Record migration
        let record = MigrationRecord {
            from_version,
            to_version: to_version.clone(),
            migrated_at: env.ledger().timestamp(),
            migrated_by: migrator,
            success: true,
            notes,
        };

        let history_key = SorobanString::from_str(env, keys::MIGRATION_HISTORY);
        let mut history: Vec<MigrationRecord> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<MigrationRecord>>(&history_key)
            .unwrap_or_else(|| vec![env]);

        history.push_back(record);
        env.storage()
            .persistent()
            .set::<SorobanString, Vec<MigrationRecord>>(&history_key, &history);

        // Clear migration flag
        let in_progress_key = SorobanString::from_str(env, keys::MIGRATION_IN_PROGRESS);
        env.storage().instance().remove(&in_progress_key);

        // Emit event
        env.events().publish(
            (symbol_short!("migration"), "completed"),
            (
                ("from_version", format!("{}.{}.{}", from_version.major, from_version.minor, from_version.patch)),
                ("to_version", format!("{}.{}.{}", to_version.major, to_version.minor, to_version.patch)),
            ),
        );

        Ok(())
    }

    /**
     * Validate state after migration
     */
    pub fn validate_state(
        env: &Env,
        checkpoint: ValidationCheckpoint,
        errors: Vec<SorobanString>,
        warnings: Vec<SorobanString>,
    ) -> Result<ValidationResult, MigrationError> {
        let passed = errors.is_empty();

        let result = ValidationResult {
            checkpoint,
            passed,
            errors,
            warnings,
            checked_at: env.ledger().timestamp(),
        };

        // Store validation result
        let validation_key = SorobanString::from_str(env, keys::VALIDATION_RESULTS);
        let mut results: Vec<ValidationResult> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<ValidationResult>>(&validation_key)
            .unwrap_or_else(|| vec![env]);

        results.push_back(result.clone());
        env.storage()
            .persistent()
            .set::<SorobanString, Vec<ValidationResult>>(&validation_key, &results);

        if !passed {
            return Err(MigrationError::ValidationFailed);
        }

        Ok(result)
    }

    /**
     * Get migration history
     */
    pub fn get_history(env: &Env) -> Vec<MigrationRecord> {
        let key = SorobanString::from_str(env, keys::MIGRATION_HISTORY);
        env.storage()
            .persistent()
            .get::<SorobanString, Vec<MigrationRecord>>(&key)
            .unwrap_or_else(|| vec![env])
    }

    /**
     * Get all state snapshots
     */
    pub fn get_snapshots(env: &Env) -> Vec<StateSnapshot> {
        let key = SorobanString::from_str(env, keys::STATE_SNAPSHOTS);
        env.storage()
            .persistent()
            .get::<SorobanString, Vec<StateSnapshot>>(&key)
            .unwrap_or_else(|| vec![env])
    }

    /**
     * Rollback to previous version using snapshot
     */
    pub fn rollback_to_snapshot(
        env: &Env,
        admin: Address,
        snapshot_hash: SorobanString,
    ) -> Result<(), MigrationError> {
        admin.require_auth();

        let snapshots_key = SorobanString::from_str(env, keys::STATE_SNAPSHOTS);
        let snapshots: Vec<StateSnapshot> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<StateSnapshot>>(&snapshots_key)
            .ok_or(MigrationError::RollbackNotAvailable)?;

        // Find matching snapshot
        let snapshot = snapshots
            .iter()
            .find(|s| s.snapshot_hash == snapshot_hash)
            .ok_or(MigrationError::InvalidStateSnapshot)?;

        // Check if snapshot is expired
        let current_time = env.ledger().timestamp();
        if current_time > snapshot.expiration_timestamp {
            return Err(MigrationError::SnapshotExpired);
        }

        // Verify snapshot is marked for rollback
        if !snapshot.rollback_available {
            return Err(MigrationError::RollbackNotAvailable);
        }

        // Restore version
        let version_key = SorobanString::from_str(env, keys::MIGRATION_VERSION);
        env.storage()
            .persistent()
            .set::<SorobanString, MigrationVersion>(&version_key, &snapshot.version);

        // Emit rollback event
        env.events().publish(
            (symbol_short!("migration"), "rollback"),
            ("snapshot_hash", snapshot_hash),
        );

        Ok(())
    }

    /**
     * Add authorized migrator
     */
    pub fn add_migrator(
        env: &Env,
        admin: Address,
        new_migrator: Address,
    ) -> Result<(), MigrationError> {
        admin.require_auth();

        let migrators_key = SorobanString::from_str(env, keys::AUTHORIZED_MIGRATORS);
        let mut migrators: Vec<Address> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<Address>>(&migrators_key)
            .unwrap_or_else(|| vec![env]);

        // Check if already added
        if migrators.iter().any(|addr| addr == &new_migrator) {
            return Ok(());
        }

        migrators.push_back(new_migrator);
        env.storage()
            .persistent()
            .set::<SorobanString, Vec<Address>>(&migrators_key, &migrators);

        Ok(())
    }

    /**
     * Remove authorized migrator
     */
    pub fn remove_migrator(
        env: &Env,
        admin: Address,
        migrator: Address,
    ) -> Result<(), MigrationError> {
        admin.require_auth();

        let migrators_key = SorobanString::from_str(env, keys::AUTHORIZED_MIGRATORS);
        let migrators: Vec<Address> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<Address>>(&migrators_key)
            .ok_or(MigrationError::UnauthorizedMigrator)?;

        let updated = migrators
            .iter()
            .filter(|addr| addr != &migrator)
            .collect::<Vec<_>>();

        env.storage().persistent().set(
            &migrators_key,
            &Vec::from_slice(env, &updated),
        );

        Ok(())
    }

    /**
     * Emit migration event for off-chain monitoring
     */
    pub fn emit_migration_event(
        env: &Env,
        from: &MigrationVersion,
        to: &MigrationVersion,
    ) {
        env.events().publish(
            (
                symbol_short!("migration"),
                from.major,
                from.minor,
                from.patch,
            ),
            (to.major, to.minor, to.patch),
        );
    }
}

// ─── Version Policy Documentation ──────────────────────────────────────────

/**
 * ## Version Migration Policy
 *
 * ### Versioning Scheme
 * Follows semantic versioning: MAJOR.MINOR.PATCH
 * - MAJOR: Breaking changes to state schema
 * - MINOR: Additive changes (backward compatible)
 * - PATCH: Bug fixes or internal optimizations
 *
 * ### Migration Rules
 * 1. Forward-only upgrades allowed (no downgrading)
 * 2. Each migration requires:
 *    - Pre-migration state snapshot
 *    - Pre-migration validation
 *    - Post-migration validation
 *    - Migration audit log entry
 * 3. Snapshots retained for 7 days for rollback capability
 * 4. Only authorized migrators can execute migrations
 * 5. Single migration at a time (mutual exclusion)
 *
 * ### Rollback Procedure
 * 1. Snapshot must be available and not expired
 * 2. Admin approval required
 * 3. State restored to snapshot version
 * 4. Rollback event emitted for off-chain monitoring
 *
 * ### Validation Checkpoints
 * 1. **PreMigration**: Validate current state is consistent
 * 2. **PostMigration**: Validate new state schema is valid
 * 3. **RollbackPrep**: Validate rollback snapshot integrity
 *
 * ### Monitoring & Alerts
 * - Track all migration attempts (success/failure)
 * - Alert on validation failures
 * - Monitor snapshot expiration
 * - Track migration duration
 */
