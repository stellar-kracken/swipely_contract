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

/// Outcome of a simulated migration produced by `dry_run_migration`. Never
/// persisted to storage — purely informational.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DryRunReport {
    pub current_version: MigrationVersion,
    pub target_version: MigrationVersion,
    /// `true` only if `issues` is empty, i.e. calling `begin_migration` with
    /// the same arguments right now would succeed.
    pub would_succeed: bool,
    /// Every problem that would block the real migration, in check order.
    pub issues: Vec<SorobanString>,
    /// Non-blocking notes (e.g. caller-supplied storage-layout warnings).
    pub warnings: Vec<SorobanString>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    AlreadyAtVersion,
    VersionDowngradeNotAllowed,
    /// The target skips one or more entire major versions (e.g. 1.x -> 3.x)
    /// without passing `force`. See `VERSION_MIGRATION_POLICY.md`.
    VersionSkipNotAllowed,
    UnauthorizedMigrator,
    ValidationFailed,
    RollbackNotAvailable,
    InvalidStateSnapshot,
    SnapshotExpired,
    NoValidationResults,
    MigrationInProgress,
    /// `cancel_migration` was called but no migration is in progress.
    NoMigrationInProgress,
    StateIntegrityCheckFailed,
}

// ─── Storage Keys ──────────────────────────────────────────────────────────

mod keys {
    pub const MIGRATION_VERSION: &str = "mig_version";
    pub const MIGRATION_HISTORY: &str = "mig_history";
    pub const STATE_SNAPSHOTS: &str = "state_snapshots";
    pub const VALIDATION_RESULTS: &str = "validation_results";
    pub const MIGRATION_IN_PROGRESS: &str = "mig_in_progress";
    // Reserved storage keys for planned features not yet wired up.
    #[allow(dead_code)]
    pub const LAST_MIGRATION_TIMESTAMP: &str = "last_mig_timestamp";
    pub const AUTHORIZED_MIGRATORS: &str = "auth_migrators";
    pub const MIGRATION_TIMEOUT_SECONDS: &str = "mig_timeout";
    #[allow(dead_code)]
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
     *
     * Rules (see `VERSION_MIGRATION_POLICY.md`):
     * - `to` must not equal `from` (`AlreadyAtVersion`, regardless of `force`).
     * - `to` must be strictly greater than `from` in semver order, unless
     *   `force` is `true` (otherwise `VersionDowngradeNotAllowed`).
     * - `to.major` must not be more than one greater than `from.major`,
     *   unless `force` is `true` (otherwise `VersionSkipNotAllowed`).
     */
    pub fn validate_upgrade(
        from: &MigrationVersion,
        to: &MigrationVersion,
        force: bool,
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

        if !is_forward && !force {
            return Err(MigrationError::VersionDowngradeNotAllowed);
        }

        if is_forward && to.major > from.major + 1 && !force {
            return Err(MigrationError::VersionSkipNotAllowed);
        }

        Ok(())
    }

    fn is_authorized_migrator(env: &Env, migrator: &Address) -> Result<bool, MigrationError> {
        let migrators_key = SorobanString::from_str(env, keys::AUTHORIZED_MIGRATORS);
        let migrators: Vec<Address> = env
            .storage()
            .persistent()
            .get::<SorobanString, Vec<Address>>(&migrators_key)
            .ok_or(MigrationError::UnauthorizedMigrator)?;
        Ok(migrators.iter().any(|addr| addr == *migrator))
    }

    fn is_migration_in_progress(env: &Env) -> bool {
        let in_progress_key = SorobanString::from_str(env, keys::MIGRATION_IN_PROGRESS);
        env.storage()
            .instance()
            .get::<SorobanString, bool>(&in_progress_key)
            .unwrap_or(false)
    }

    /**
     * Begin migration with safety checks
     *
     * Validates the target version via `validate_upgrade` (pass `force` to
     * override a rejected downgrade or major-version skip), then sets the
     * mutual-exclusion flag. If the migration doesn't complete (the caller
     * never calls `complete_migration`), use `cancel_migration` to clear the
     * flag and unblock future migrations.
     */
    pub fn begin_migration(
        env: &Env,
        migrator: Address,
        target_version: MigrationVersion,
        force: bool,
    ) -> Result<(), MigrationError> {
        migrator.require_auth();

        if !Self::is_authorized_migrator(env, &migrator)? {
            return Err(MigrationError::UnauthorizedMigrator);
        }

        if Self::is_migration_in_progress(env) {
            return Err(MigrationError::MigrationInProgress);
        }

        let current_version = Self::get_version(env);
        Self::validate_upgrade(&current_version, &target_version, force)?;

        // Set migration flag
        let in_progress_key = SorobanString::from_str(env, keys::MIGRATION_IN_PROGRESS);
        env.storage().instance().set(&in_progress_key, &true);

        Ok(())
    }

    /**
     * Cancel a stuck migration — the recovery path when a migration was
     * begun (via `begin_migration`) but never completed, e.g. because the
     * off-chain migration logic between the two calls failed. Clears the
     * mutual-exclusion flag so a fresh `begin_migration` can proceed. Does
     * not touch the stored version or history; pair with
     * `rollback_to_snapshot` if the partial migration also needs its state
     * reverted.
     */
    pub fn cancel_migration(env: &Env, migrator: Address) -> Result<(), MigrationError> {
        migrator.require_auth();

        if !Self::is_authorized_migrator(env, &migrator)? {
            return Err(MigrationError::UnauthorizedMigrator);
        }

        if !Self::is_migration_in_progress(env) {
            return Err(MigrationError::NoMigrationInProgress);
        }

        let in_progress_key = SorobanString::from_str(env, keys::MIGRATION_IN_PROGRESS);
        env.storage().instance().remove(&in_progress_key);

        env.events()
            .publish((symbol_short!("migration"), "cancelled"), migrator);

        Ok(())
    }

    /**
     * Simulate a migration to `target_version` without mutating any
     * contract state: checks authorization, mutual exclusion, and the
     * version policy (the same checks `begin_migration` would run with the
     * same `force` value), folds in any caller-supplied storage-layout
     * `errors`/`warnings` (see `validate_state`), and reports every problem
     * found instead of stopping at the first one.
     */
    pub fn dry_run_migration(
        env: &Env,
        migrator: Address,
        target_version: MigrationVersion,
        storage_errors: Vec<SorobanString>,
        storage_warnings: Vec<SorobanString>,
        force: bool,
    ) -> DryRunReport {
        let current_version = Self::get_version(env);
        let mut issues = Vec::new(env);

        match Self::is_authorized_migrator(env, &migrator) {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                issues.push_back(SorobanString::from_str(
                    env,
                    "migrator is not in the authorized-migrators list",
                ));
            }
        }

        if Self::is_migration_in_progress(env) {
            issues.push_back(SorobanString::from_str(
                env,
                "a migration is already in progress",
            ));
        }

        match Self::validate_upgrade(&current_version, &target_version, force) {
            Ok(()) => {}
            Err(MigrationError::AlreadyAtVersion) => {
                issues.push_back(SorobanString::from_str(
                    env,
                    "target version equals the current version",
                ));
            }
            Err(MigrationError::VersionDowngradeNotAllowed) => {
                issues.push_back(SorobanString::from_str(
                    env,
                    "target version is a downgrade (pass force to override)",
                ));
            }
            Err(MigrationError::VersionSkipNotAllowed) => {
                issues.push_back(SorobanString::from_str(
                    env,
                    "target version skips one or more major versions (pass force to override)",
                ));
            }
            Err(_) => {}
        }

        for err in storage_errors.iter() {
            issues.push_back(err);
        }

        let would_succeed = issues.is_empty();

        DryRunReport {
            current_version,
            target_version,
            would_succeed,
            issues,
            warnings: storage_warnings,
        }
    }

    /**
     * Record migration completion
     *
     * Pass the same `force` value used at `begin_migration` so a migration
     * that was legitimately begun as a forced downgrade/skip doesn't get
     * rejected again here.
     */
    pub fn complete_migration(
        env: &Env,
        from_version: MigrationVersion,
        to_version: MigrationVersion,
        migrator: Address,
        notes: SorobanString,
        force: bool,
    ) -> Result<(), MigrationError> {
        // Validate upgrade path
        Self::validate_upgrade(&from_version, &to_version, force)?;

        // Update current version
        let version_key = SorobanString::from_str(env, keys::MIGRATION_VERSION);
        env.storage()
            .persistent()
            .set::<SorobanString, MigrationVersion>(&version_key, &to_version);

        // Record migration
        let record = MigrationRecord {
            from_version: from_version.clone(),
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
                (
                    "from_version",
                    (from_version.major, from_version.minor, from_version.patch),
                ),
                (
                    "to_version",
                    (to_version.major, to_version.minor, to_version.patch),
                ),
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
        if migrators.iter().any(|addr| addr == new_migrator) {
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

        let mut updated = Vec::new(env);
        for addr in migrators.iter() {
            if addr != migrator {
                updated.push_back(addr);
            }
        }

        env.storage()
            .persistent()
            .set::<SorobanString, Vec<Address>>(&migrators_key, &updated);

        Ok(())
    }

    /**
     * Emit migration event for off-chain monitoring
     */
    pub fn emit_migration_event(env: &Env, from: &MigrationVersion, to: &MigrationVersion) {
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

/*
 * ## Version Migration Policy
 *
 * ### Versioning Scheme
 * Follows semantic versioning: MAJOR.MINOR.PATCH
 * - MAJOR: Breaking changes to state schema
 * - MINOR: Additive changes (backward compatible)
 * - PATCH: Bug fixes or internal optimizations
 *
 * ### Migration Rules
 * 1. Forward-only upgrades allowed (no downgrading, no skipping an entire
 *    major version) unless the caller explicitly passes `force = true`
 * 2. Each migration requires:
 *    - Pre-migration state snapshot
 *    - Pre-migration validation
 *    - Post-migration validation
 *    - Migration audit log entry
 * 3. Snapshots retained for 7 days for rollback capability
 * 4. Only authorized migrators can execute migrations
 * 5. Single migration at a time (mutual exclusion) — use `dry_run_migration`
 *    beforehand to preview whether a call would succeed without mutating
 *    state, and `cancel_migration` to clear a stuck in-progress flag if a
 *    migration fails partway between `begin_migration` and
 *    `complete_migration`
 *
 * ### Rollback Procedure
 * 1. Snapshot must be available and not expired
 * 2. Admin approval required
 * 3. State restored to snapshot version
 * 4. Rollback event emitted for off-chain monitoring
 *
 * ### Recovery From a Partial Migration
 * If `begin_migration` succeeds but `complete_migration` is never called
 * (the off-chain migration logic failed or crashed), the contract is left
 * with its mutual-exclusion flag set and no version change:
 * 1. Call `cancel_migration` to clear the flag (any authorized migrator)
 * 2. If the partial migration already mutated other contract state, restore
 *    it from the pre-migration snapshot via `rollback_to_snapshot`
 * 3. Investigate before retrying `begin_migration`
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
