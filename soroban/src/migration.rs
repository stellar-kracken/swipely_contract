#![allow(dead_code)]

use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec, vec};

// Storage key constants for migration state — follows the same pattern as the
// top-level `keys` mod in lib.rs.
mod keys {
    pub const MIGRATION_VERSION: &str = "mig_version";
    pub const MIGRATION_HISTORY: &str = "mig_history";
}

/// Semantic version of the on-chain contract state schema.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// Immutable record written to persistent storage after every migration attempt.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRecord {
    pub from_version: MigrationVersion,
    pub to_version: MigrationVersion,
    /// Ledger timestamp (Unix seconds) at the time the migration ran.
    pub migrated_at: u64,
    /// Address that triggered the migration.
    pub migrated_by: Address,
    /// Whether the migration completed without errors.
    pub success: bool,
    /// Optional human-readable notes, e.g. what was changed.
    pub notes: String,
}

/// Errors that can be returned by migration validation logic.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    /// The contract state is already at the requested target version.
    AlreadyAtVersion,
    /// Downgrading the schema version is not permitted.
    VersionDowngradeNotAllowed,
    /// The caller is not authorised to run migrations.
    UnauthorizedMigrator,
    /// Pre- or post-migration validation checks failed.
    ValidationFailed,
    /// No rollback snapshot is available for the requested version.
    RollbackNotAvailable,
}

/// Stateless helper that manages reading, writing, and auditing contract state
/// versions.  All state is stored in the environment's persistent storage so
/// that it survives contract upgrades.
pub struct MigrationHelper;

impl MigrationHelper {
    /// Read the current schema version from persistent storage.
    /// Returns `MigrationVersion { major: 0, minor: 0, patch: 0 }` when no
    /// version has been written yet (fresh deployment).
    pub fn get_version(env: &Env) -> MigrationVersion {
        let key = String::from_str(env, keys::MIGRATION_VERSION);
        env.storage()
            .persistent()
            .get::<String, MigrationVersion>(&key)
            .unwrap_or(MigrationVersion {
                major: 0,
                minor: 0,
                patch: 0,
            })
    }

    /// Persist `version` as the current schema version.
    pub fn set_version(env: &Env, version: MigrationVersion) {
        let key = String::from_str(env, keys::MIGRATION_VERSION);
        env.storage().persistent().set::<String, MigrationVersion>(&key, &version);
    }

    /// Validate that migrating `from` → `to` is a forward-only upgrade.
    ///
    /// Rules:
    /// - `to` must not equal `from` (that would be `AlreadyAtVersion`).
    /// - `to` must be strictly greater than `from` in semver order (major
    ///   takes precedence, then minor, then patch).  Any lower value is
    ///   `VersionDowngradeNotAllowed`.
    pub fn validate_upgrade(
        from: &MigrationVersion,
        to: &MigrationVersion,
    ) -> Result<(), MigrationError> {
        if from == to {
            return Err(MigrationError::AlreadyAtVersion);
        }

        // Compare (major, minor, patch) lexicographically.
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

    /// Append `record` to the persistent migration history log.
    ///
    /// The history is stored as a `Vec<MigrationRecord>`.  If no history
    /// exists yet a new single-element vector is created.
    pub fn record_migration(env: &Env, record: MigrationRecord) {
        let key = String::from_str(env, keys::MIGRATION_HISTORY);
        let mut history: Vec<MigrationRecord> = env
            .storage()
            .persistent()
            .get::<String, Vec<MigrationRecord>>(&key)
            .unwrap_or_else(|| vec![env]);

        history.push_back(record);
        env.storage()
            .persistent()
            .set::<String, Vec<MigrationRecord>>(&key, &history);
    }

    /// Return the full migration history in insertion order (oldest first).
    pub fn get_history(env: &Env) -> Vec<MigrationRecord> {
        let key = String::from_str(env, keys::MIGRATION_HISTORY);
        env.storage()
            .persistent()
            .get::<String, Vec<MigrationRecord>>(&key)
            .unwrap_or_else(|| vec![env])
    }

    /// Publish a contract event so off-chain indexers can observe migrations.
    ///
    /// Event topics: `["migration", from_major, from_minor, from_patch]`
    /// Event data:   `[to_major, to_minor, to_patch]`
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
