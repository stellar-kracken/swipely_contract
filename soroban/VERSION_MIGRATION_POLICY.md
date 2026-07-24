# Version Migration Helper - Documentation

## Overview

The **Version Migration Helper** is a comprehensive system for managing safe contract state migrations between versions. It provides:

- **Version Tracking**: Semantic versioning support (MAJOR.MINOR.PATCH)
- **Migration Validation**: Pre and post-migration state verification
- **Dry-Run Simulation**: Preview a migration's outcome — authorization,
  mutual exclusion, version policy, and storage-layout checks — without
  writing anything to storage
- **Downgrade/Skip Guard**: Downgrades and multi-major-version skips are
  rejected unless explicitly forced
- **Rollback Support**: Snapshot-based state rollback capability
- **Stuck-Migration Recovery**: Cancel a migration that was begun but never
  completed, clearing the mutual-exclusion flag
- **Audit Trail**: Complete migration history with timestamps and actor tracking
- **Authorization Control**: Role-based migration authorization
- **State Verification**: Integrity checks before and after migration

## Key Features

### 1. Semantic Versioning

```
Version Format: MAJOR.MINOR.PATCH
  - MAJOR: Breaking changes to state schema
  - MINOR: Additive changes (backward compatible)
  - PATCH: Bug fixes or internal optimizations
```

### 2. Migration Rules

- **Forward-only upgrades**: No downgrading to previous versions, enforced by
  `validate_upgrade` and checked again by `begin_migration`
- **No version skipping**: Jumping more than one MAJOR version in a single
  migration (e.g. `1.x -> 3.x`) is rejected; migrate through each MAJOR
  version in sequence
- **Force override**: Both of the above are policy defaults, not hard limits
  — pass `force = true` to `validate_upgrade` / `begin_migration` /
  `complete_migration` to explicitly override a downgrade or version skip
  when that's genuinely intended (e.g. a manual state correction)
- **Single migration at a time**: Mutual exclusion prevents concurrent migrations
- **Authorization required**: Only designated migrators can execute migrations
- **Validation required**: Pre and post-migration validation checkpoints
- **Dry-run first**: `dry_run_migration` simulates a `begin_migration` call —
  same authorization, mutual-exclusion, and version-policy checks, plus any
  caller-supplied storage-layout checks — and reports every problem found,
  without writing anything to storage

### 3. State Snapshots

- Created before migration begins
- Retained for 7 days (configurable)
- Used for rollback in case of issues
- Include full state data and hash for integrity verification

### 4. Rollback Support

```
Rollback Process:
1. Verify snapshot exists and is not expired
2. Admin authorization required
3. Restore contract state to snapshot version
4. Emit rollback event for off-chain monitoring
```

### 5. Stuck-Migration Recovery

If `begin_migration` succeeds but the migration never reaches
`complete_migration` (the off-chain migration logic failed or crashed
partway through), the contract is left with its mutual-exclusion flag set
and no version change. `cancel_migration` clears that flag — see
[Recovering From a Partial Migration](#recovering-from-a-partial-migration).

## API Reference

### Initialization

```rust
pub fn initialize(
    env: &Env,
    admin: Address,
    initial_version: MigrationVersion,
) -> Result<(), MigrationError>
```

Initialize the migration system with an initial version.

### Get Current Version

```rust
pub fn get_version(env: &Env) -> MigrationVersion
```

Retrieve the current contract version.

### Validate Upgrade Path

```rust
pub fn validate_upgrade(
    from: &MigrationVersion,
    to: &MigrationVersion,
    force: bool,
) -> Result<(), MigrationError>
```

Validate that an upgrade from one version to another is allowed: `to` must
be strictly forward of `from` and must not skip an entire MAJOR version.
Pass `force = true` to bypass both checks (an equal `from`/`to` is always
rejected, `force` or not).

### Dry-Run a Migration

```rust
pub fn dry_run_migration(
    env: &Env,
    migrator: Address,
    target_version: MigrationVersion,
    storage_errors: Vec<SorobanString>,
    storage_warnings: Vec<SorobanString>,
    force: bool,
) -> DryRunReport
```

Simulate calling `begin_migration` with the same arguments, without
mutating any contract state: checks authorization, mutual exclusion, and
the version policy, folds in caller-supplied storage-layout
`storage_errors`/`storage_warnings` (the same shape `validate_state`
takes), and returns a `DryRunReport { current_version, target_version,
would_succeed, issues, warnings }` listing every problem found rather than
stopping at the first one.

### Begin Migration

```rust
pub fn begin_migration(
    env: &Env,
    migrator: Address,
    target_version: MigrationVersion,
    force: bool,
) -> Result<(), MigrationError>
```

Validates `target_version` via `validate_upgrade` (pass `force` to
override a rejected downgrade or major-version skip), then sets the
mutual-exclusion flag.

### Cancel a Stuck Migration

```rust
pub fn cancel_migration(env: &Env, migrator: Address) -> Result<(), MigrationError>
```

Clears the mutual-exclusion flag left by a `begin_migration` that never
reached `complete_migration`. Callable by any authorized migrator; fails
with `NoMigrationInProgress` if there's nothing to cancel. See
[Recovering From a Partial Migration](#recovering-from-a-partial-migration).

### Complete Migration

```rust
pub fn complete_migration(
    env: &Env,
    from_version: MigrationVersion,
    to_version: MigrationVersion,
    migrator: Address,
    notes: SorobanString,
    force: bool,
) -> Result<(), MigrationError>
```

Record migration completion and update contract version. Pass the same
`force` value used at `begin_migration` so a legitimately-forced
downgrade/skip isn't rejected again here.

### Create State Snapshot

```rust
pub fn create_state_snapshot(
    env: &Env,
    snapshot_by: Address,
    description: SorobanString,
    data: Map<SorobanString, SorobanString>,
) -> Result<StateSnapshot, MigrationError>
```

Create a snapshot of current contract state for rollback capability.

### Validate State

```rust
pub fn validate_state(
    env: &Env,
    checkpoint: ValidationCheckpoint,
    errors: Vec<SorobanString>,
    warnings: Vec<SorobanString>,
) -> Result<ValidationResult, MigrationError>
```

Validate contract state at a specific checkpoint.

### Rollback to Snapshot

```rust
pub fn rollback_to_snapshot(
    env: &Env,
    admin: Address,
    snapshot_hash: SorobanString,
) -> Result<(), MigrationError>
```

Restore contract state to a previous snapshot.

### Authorization Management

```rust
pub fn add_migrator(
    env: &Env,
    admin: Address,
    new_migrator: Address,
) -> Result<(), MigrationError>

pub fn remove_migrator(
    env: &Env,
    admin: Address,
    migrator: Address,
) -> Result<(), MigrationError>
```

Manage authorized migration users.

### Query Functions

```rust
pub fn get_history(env: &Env) -> Vec<MigrationRecord>
pub fn get_snapshots(env: &Env) -> Vec<StateSnapshot>
```

Retrieve migration history and available snapshots.

## Migration Workflow

### Standard Migration Process

```
1. Create Pre-Migration Snapshot
   ├─ Capture current state
   ├─ Calculate state hash
   └─ Store for rollback

2. Begin Migration
   ├─ Verify authorization
   ├─ Check target version is valid
   ├─ Set mutual exclusion flag
   └─ Emit start event

3. Pre-Migration Validation
   ├─ Verify current state consistency
   ├─ Check required data fields
   └─ Report any issues

4. Execute Migration Logic
   ├─ Transform state to new schema
   ├─ Add/remove/modify data structures
   └─ Maintain data integrity

5. Post-Migration Validation
   ├─ Verify new state structure
   ├─ Check all required fields present
   ├─ Validate data consistency
   └─ Report any issues

6. Complete Migration
   ├─ Update version number
   ├─ Record migration in history
   ├─ Clear mutual exclusion flag
   └─ Emit completion event
```

### Rollback Process

```
1. Admin Authorization
   └─ Verify admin signature

2. Snapshot Selection
   └─ Find valid, non-expired snapshot

3. State Restoration
   ├─ Restore contract state
   ├─ Update version to snapshot version
   └─ Emit rollback event

4. Verification
   └─ Manual verification recommended
```

## Validation Checkpoints

### PreMigration Checkpoint
- Verify current state is consistent
- Check all required storage keys are present
- Validate data types and ranges
- Report missing or corrupted data

### PostMigration Checkpoint
- Verify new state structure
- Check all new fields are properly initialized
- Validate schema version is updated
- Verify no data was lost

### RollbackPrep Checkpoint
- Verify snapshot integrity
- Check snapshot hash validity
- Confirm snapshot is not expired
- Ensure rollback data is complete

## Error Handling

### MigrationError Types

```rust
pub enum MigrationError {
    // Version is already at target version
    AlreadyAtVersion,
    
    // Attempted to downgrade to older version
    VersionDowngradeNotAllowed,
    
    // Target skips one or more entire major versions (e.g. 1.x -> 3.x)
    VersionSkipNotAllowed,
    
    // Caller is not authorized to migrate
    UnauthorizedMigrator,
    
    // Pre or post-migration validation failed
    ValidationFailed,
    
    // No valid rollback snapshot available
    RollbackNotAvailable,
    
    // Snapshot is invalid or corrupted
    InvalidStateSnapshot,
    
    // Snapshot has expired
    SnapshotExpired,
    
    // No validation results found
    NoValidationResults,
    
    // Another migration is already in progress
    MigrationInProgress,
    
    // cancel_migration was called but no migration is in progress
    NoMigrationInProgress,
    
    // State integrity check failed
    StateIntegrityCheckFailed,
}
```

## Events

### Migration Events

```
Event: migration_completed
├─ from_version: String (e.g., "1.0.0")
└─ to_version: String (e.g., "1.1.0")

Event: migration_rollback
├─ snapshot_hash: String
└─ timestamp: u64

Event: migration_initiated
├─ target_version: String
└─ timestamp: u64

Event: migration_cancelled
└─ migrator: Address
```

## Best Practices

### 1. Pre-Migration Planning
- Document all state schema changes
- Identify all affected storage keys
- Plan data transformation logic
- Prepare rollback procedures

### 2. Testing
- Test migrations in staging environment first
- Verify all validation checkpoints
- Test rollback procedures
- Validate data integrity after migration

### 3. Monitoring
- Monitor migration event emissions
- Track migration duration
- Alert on validation failures
- Monitor snapshot expiration

### 4. Authorization
- Limit migrator authorization to essential personnel
- Use role-based access control
- Audit all migration executions
- Review migration history regularly

### 5. Documentation
- Document each version's schema
- Keep detailed migration notes
- Maintain rollback procedures
- Update deployment runbooks

## Example: Simple Migration

```rust
// Initialize system
EnhancedMigrationHelper::initialize(
    &env,
    admin_address,
    MigrationVersion { major: 1, minor: 0, patch: 0 }
)?;

// Create snapshot before migration
let snapshot = EnhancedMigrationHelper::create_state_snapshot(
    &env,
    admin_address,
    SorobanString::from_str(&env, "Pre-1.1.0 migration"),
    state_data
)?;

// Preview the migration first — no storage is written by this call
let report = EnhancedMigrationHelper::dry_run_migration(
    &env,
    admin_address,
    MigrationVersion { major: 1, minor: 1, patch: 0 },
    Vec::new(&env), // storage_errors: results of caller-side layout checks
    Vec::new(&env), // storage_warnings
    false,          // force
);
assert!(report.would_succeed);

// Begin migration
EnhancedMigrationHelper::begin_migration(
    &env,
    admin_address,
    MigrationVersion { major: 1, minor: 1, patch: 0 },
    false, // force
)?;

// Pre-migration validation
EnhancedMigrationHelper::validate_state(
    &env,
    ValidationCheckpoint::PreMigration,
    errors,
    warnings
)?;

// Execute migration logic
// ... application-specific migration code ...

// Post-migration validation
EnhancedMigrationHelper::validate_state(
    &env,
    ValidationCheckpoint::PostMigration,
    errors,
    warnings
)?;

// Complete migration
EnhancedMigrationHelper::complete_migration(
    &env,
    MigrationVersion { major: 1, minor: 0, patch: 0 },
    MigrationVersion { major: 1, minor: 1, patch: 0 },
    admin_address,
    SorobanString::from_str(&env, "Migration completed successfully"),
    false, // force
)?;

// If step 3 or 4 above fails or crashes before complete_migration runs,
// recover with:
// EnhancedMigrationHelper::cancel_migration(&env, admin_address)?;
```

## Monitoring & Alerting

### Key Metrics to Monitor

1. **Migration Success Rate**
   - Track successful vs failed migrations
   - Alert on unusual failure patterns

2. **Migration Duration**
   - Track how long each migration takes
   - Alert if migration takes longer than expected

3. **Snapshot Health**
   - Monitor snapshot creation success
   - Alert on snapshot expiration events

4. **Validation Status**
   - Track validation checkpoint results
   - Alert on validation failures

### Example Alert Rules

```
// Alert if migration fails
trigger: migration_failed
alert: "Contract migration failed from v{from} to v{to}"

// Alert if snapshot expires without rollback
trigger: snapshot_expiration
alert: "Snapshot {snapshot_id} expired without being used for rollback"

// Alert if validation fails
trigger: validation_failed
alert: "Migration validation failed at {checkpoint}: {errors}"

// Alert if migration takes too long
trigger: migration_duration > 5_minutes
alert: "Migration taking longer than expected"
```

## Troubleshooting

### Issue: Migration Fails at Pre-Migration Validation

**Solution:**
1. Verify contract state consistency
2. Check for missing storage keys
3. Review validation error messages
4. Inspect raw storage data
5. Consider rolling back and investigating

### Issue: Post-Migration Validation Fails

**Solution:**
1. Verify migration logic executed correctly
2. Check new state structure
3. Validate data transformation
4. Compare with expected schema
5. Consider rolling back

### Issue: Cannot Rollback - Snapshot Expired

**Solution:**
1. Create new snapshot before migrations
2. Increase snapshot retention period
3. If no snapshot available, manual state correction required
4. Review snapshot management policies

### Issue: Migration In Progress (Mutual Exclusion)

**Solution:**
1. Verify the previous migration actually completed (check `get_history`)
2. If it didn't — the off-chain migration logic failed or crashed between
   `begin_migration` and `complete_migration` — call `cancel_migration` (any
   authorized migrator) to clear the stuck flag
3. If the partial migration already mutated other contract state beyond the
   version, restore it from the pre-migration snapshot via
   `rollback_to_snapshot` before retrying
4. Monitor migration completion carefully; watch for the `migration_cancelled`
   event as a signal that a migration didn't complete cleanly

### Recovering From a Partial Migration

The full recovery procedure for a migration that fails partway through:

1. **Detect**: `begin_migration` on the next attempt returns
   `MigrationInProgress` instead of succeeding
2. **Cancel**: call `cancel_migration(env, migrator)` as any authorized
   migrator to clear the mutual-exclusion flag (fails with
   `NoMigrationInProgress` if nothing was actually stuck)
3. **Restore state, if needed**: if the failed migration changed contract
   state beyond the version number, call `rollback_to_snapshot` with the
   pre-migration snapshot's hash to restore it
4. **Investigate**: review `get_history` and the `migration_cancelled` event
   before retrying `begin_migration`

## Version Policy

### Backward Compatibility

- **PATCH versions**: Always backward compatible
- **MINOR versions**: Backward compatible (additive only)
- **MAJOR versions**: May have breaking changes

### Upgrade Strategy

- **Within same MAJOR**: Use automatic migration
- **Across MAJOR versions**: Plan careful multi-step migrations
- **Skip versions**: Enforced in code, not just convention — `validate_upgrade`
  rejects a target more than one MAJOR version ahead (`VersionSkipNotAllowed`);
  migrate through each MAJOR version in sequence, or pass `force = true` if
  skipping is genuinely intended

### Deprecation

- Document deprecated features at least 2 versions ahead
- Remove deprecated features only in MAJOR versions
- Provide migration guidance for all breaking changes
