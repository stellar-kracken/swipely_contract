# Error Code Reference

Every `#[contracterror]` numeric error code defined under `soroban/src/`, grouped by the contract that defines it. Soroban scopes error codes to the specific `#[contracterror]` enum a function returns — two contracts using the value `3` for different things is not a collision in the way it would be in a single flat global error space, since a caller always decodes the code against the ABI of the specific contract/function they invoked. This table exists so a human cross-referencing a raw numeric code (e.g. from an indexer or a transaction result, without the originating contract's ABI in hand) has one place to look, and so a reviewer can confirm each contract's own codes are internally unique and contiguous.

## Contents

- [`RelayError`](#relayerror) — CrossChainRelayContract (Part 3 — Relay contract (test-binary only))
- [`RecoveryError`](#recoveryerror) — EmergencyFundRecovery (Part 1 — Deployed contract (production wasm))
- [`DeprecationError`](#deprecationerror) — AssetDeprecationContract (Part 4 — Standalone experimental contract (cfg(test) only))
- [`RegistryError`](#registryerror) — AssetRegistryContract (Part 4 — Standalone experimental contract (cfg(test) only))
- [`BatchQueryError`](#batchqueryerror) — BatchQueryContract (Part 4 — Standalone experimental contract (cfg(test) only))
- [`RateLimitError`](#ratelimiterror) — RateLimiterContract (Part 4 — Standalone experimental contract (cfg(test) only))
- [`SidecarError`](#sidecarerror) — SidecarStateContract (Part 4 — Standalone experimental contract (cfg(test) only))
- [`Error`](#error) — BridgeReserveVerifier (Part 5 — Unreferenced file (not compiled anywhere))

## `RelayError`

**Contract:** CrossChainRelayContract — Part 3 — Relay contract (test-binary only)  
**Source:** [`soroban/src/relay/errors.rs`](../src/relay/errors.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `AlreadyInitialized` | The contract has already been initialised. |
| 2 | `NotInitialized` | The contract has not been initialised. |
| 3 | `Unauthorized` | Caller is not the contract administrator. |
| 4 | `InvalidNonce` | The nonce supplied does not match the expected value. |
| 5 | `MessageExpired` | The message has expired. |
| 6 | `MessageNotFound` | The message was not found. |
| 7 | `InvalidMessageStatus` | The message is in an invalid state for this operation. |
| 8 | `OperatorNotActive` | The relay operator is not registered or is inactive. |
| 9 | `OperatorAlreadyRegistered` | The relay operator is already registered. |
| 10 | `InvalidSignature` | Signature verification failed. |
| 11 | `InvalidStateProof` | State proof verification failed. |
| 12 | `ChainNotEnabled` | The target chain is not enabled. |
| 13 | `ChainConfigNotFound` | The target chain configuration was not found. |
| 14 | `InsufficientFee` | Insufficient fee attached to the message. |
| 15 | `PayloadTooLarge` | The message payload exceeds the maximum allowed size. |
| 16 | `EmptyBatch` | The batch is empty. |
| 17 | `InvalidTtl` | The TTL value is invalid. |

## `RecoveryError`

**Contract:** EmergencyFundRecovery — Part 1 — Deployed contract (production wasm)  
**Source:** [`soroban/src/emergency_fund_recovery.rs`](../src/emergency_fund_recovery.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `NotAuthorized` | _(no doc comment on the variant; name reads as: not authorized)_ |
| 2 | `InvalidAmount` | _(no doc comment on the variant; name reads as: invalid amount)_ |
| 3 | `InvalidDestination` | _(no doc comment on the variant; name reads as: invalid destination)_ |
| 4 | `RecoveryNotFound` | _(no doc comment on the variant; name reads as: recovery not found)_ |
| 5 | `AlreadyApproved` | _(no doc comment on the variant; name reads as: already approved)_ |
| 6 | `InsufficientApprovals` | _(no doc comment on the variant; name reads as: insufficient approvals)_ |
| 7 | `TimelockNotElapsed` | _(no doc comment on the variant; name reads as: timelock not elapsed)_ |
| 8 | `RecoveryAlreadyExecuted` | _(no doc comment on the variant; name reads as: recovery already executed)_ |
| 9 | `RecoveryAlreadyCancelled` | _(no doc comment on the variant; name reads as: recovery already cancelled)_ |
| 10 | `InvalidRecoveryState` | _(no doc comment on the variant; name reads as: invalid recovery state)_ |
| 11 | `EmergencyModeDisabled` | _(no doc comment on the variant; name reads as: emergency mode disabled)_ |
| 12 | `NoFundsToRecover` | _(no doc comment on the variant; name reads as: no funds to recover)_ |
| 13 | `TokenTransferFailed` | _(no doc comment on the variant; name reads as: token transfer failed)_ |
| 14 | `InvalidTimelock` | _(no doc comment on the variant; name reads as: invalid timelock)_ |

## `DeprecationError`

**Contract:** AssetDeprecationContract — Part 4 — Standalone experimental contract (cfg(test) only)  
**Source:** [`soroban/src/asset_deprecation.rs`](../src/asset_deprecation.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `NotAuthorized` | _(no doc comment on the variant; name reads as: not authorized)_ |
| 2 | `AlreadyInitialized` | _(no doc comment on the variant; name reads as: already initialized)_ |
| 3 | `AssetNotFound` | _(no doc comment on the variant; name reads as: asset not found)_ |
| 4 | `AlreadyDeprecated` | _(no doc comment on the variant; name reads as: already deprecated)_ |
| 5 | `ReplacementNotFound` | _(no doc comment on the variant; name reads as: replacement not found)_ |
| 6 | `MigrationPeriodExpired` | _(no doc comment on the variant; name reads as: migration period expired)_ |
| 7 | `WriteOperationBlocked` | _(no doc comment on the variant; name reads as: write operation blocked)_ |

## `RegistryError`

**Contract:** AssetRegistryContract — Part 4 — Standalone experimental contract (cfg(test) only)  
**Source:** [`soroban/src/asset_registry.rs`](../src/asset_registry.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `NotAuthorized` | _(no doc comment on the variant; name reads as: not authorized)_ |
| 2 | `AlreadyInitialized` | _(no doc comment on the variant; name reads as: already initialized)_ |
| 3 | `AssetAlreadyRegistered` | _(no doc comment on the variant; name reads as: asset already registered)_ |
| 4 | `AssetNotFound` | _(no doc comment on the variant; name reads as: asset not found)_ |
| 5 | `InvalidAssetData` | _(no doc comment on the variant; name reads as: invalid asset data)_ |
| 6 | `InvalidRiskRating` | _(no doc comment on the variant; name reads as: invalid risk rating)_ |
| 7 | `InvalidLifecycleTransition` | _(no doc comment on the variant; name reads as: invalid lifecycle transition)_ |
| 8 | `MaxChainsExceeded` | _(no doc comment on the variant; name reads as: max chains exceeded)_ |
| 9 | `MaxOracleFeedsExceeded` | _(no doc comment on the variant; name reads as: max oracle feeds exceeded)_ |
| 10 | `MaxBridgesExceeded` | _(no doc comment on the variant; name reads as: max bridges exceeded)_ |
| 11 | `MaxPoolsExceeded` | _(no doc comment on the variant; name reads as: max pools exceeded)_ |
| 12 | `DuplicateChainLink` | _(no doc comment on the variant; name reads as: duplicate chain link)_ |
| 13 | `DuplicateOracleFeed` | _(no doc comment on the variant; name reads as: duplicate oracle feed)_ |
| 14 | `DuplicateBridge` | _(no doc comment on the variant; name reads as: duplicate bridge)_ |
| 15 | `DuplicatePool` | _(no doc comment on the variant; name reads as: duplicate pool)_ |
| 16 | `AssetPaused` | _(no doc comment on the variant; name reads as: asset paused)_ |
| 17 | `AssetDeprecated` | _(no doc comment on the variant; name reads as: asset deprecated)_ |
| 18 | `AssetNotWhitelisted` | _(no doc comment on the variant; name reads as: asset not whitelisted)_ |
| 19 | `AssetAlreadyWhitelisted` | _(no doc comment on the variant; name reads as: asset already whitelisted)_ |
| 20 | `AssetFrozen` | _(no doc comment on the variant; name reads as: asset frozen)_ |
| 21 | `AssetAlreadyActive` | Attempted to deactivate an asset that is already in a non-restorable state or already active. Deactivation is only valid for Active assets. Check the asset's current status. |
| 22 | `AssetNotDeactivated` | Attempted to restore an asset that is not in a Deactivated state. Only deactivated assets can be restored. Use the asset's current status to determine next actions. |

## `BatchQueryError`

**Contract:** BatchQueryContract — Part 4 — Standalone experimental contract (cfg(test) only)  
**Source:** [`soroban/src/batch_query.rs`](../src/batch_query.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `AlreadyInitialized` | _(no doc comment on the variant; name reads as: already initialized)_ |
| 2 | `BatchSizeExceeded` | _(no doc comment on the variant; name reads as: batch size exceeded)_ |
| 3 | `EmptyBatch` | _(no doc comment on the variant; name reads as: empty batch)_ |
| 4 | `InvalidQuery` | _(no doc comment on the variant; name reads as: invalid query)_ |

## `RateLimitError`

**Contract:** RateLimiterContract — Part 4 — Standalone experimental contract (cfg(test) only)  
**Source:** [`soroban/src/rate_limiter.rs`](../src/rate_limiter.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `NotAuthorized` | _(no doc comment on the variant; name reads as: not authorized)_ |
| 2 | `AlreadyInitialized` | _(no doc comment on the variant; name reads as: already initialized)_ |
| 3 | `DailyValueLimitExceeded` | _(no doc comment on the variant; name reads as: daily value limit exceeded)_ |
| 4 | `WeeklyValueLimitExceeded` | _(no doc comment on the variant; name reads as: weekly value limit exceeded)_ |
| 5 | `MonthlyValueLimitExceeded` | _(no doc comment on the variant; name reads as: monthly value limit exceeded)_ |
| 6 | `DailyCountLimitExceeded` | _(no doc comment on the variant; name reads as: daily count limit exceeded)_ |
| 7 | `WeeklyCountLimitExceeded` | _(no doc comment on the variant; name reads as: weekly count limit exceeded)_ |
| 8 | `MonthlyCountLimitExceeded` | _(no doc comment on the variant; name reads as: monthly count limit exceeded)_ |
| 9 | `GlobalDailyLimitExceeded` | _(no doc comment on the variant; name reads as: global daily limit exceeded)_ |
| 10 | `GlobalWeeklyLimitExceeded` | _(no doc comment on the variant; name reads as: global weekly limit exceeded)_ |
| 11 | `CooldownActive` | _(no doc comment on the variant; name reads as: cooldown active)_ |
| 12 | `CircuitBreakerTripped` | _(no doc comment on the variant; name reads as: circuit breaker tripped)_ |
| 13 | `InvalidLimit` | _(no doc comment on the variant; name reads as: invalid limit)_ |
| 14 | `InvalidRiskScore` | _(no doc comment on the variant; name reads as: invalid risk score)_ |
| 15 | `UserNotFound` | _(no doc comment on the variant; name reads as: user not found)_ |
| 16 | `EmergencyModeActive` | _(no doc comment on the variant; name reads as: emergency mode active)_ |

## `SidecarError`

**Contract:** SidecarStateContract — Part 4 — Standalone experimental contract (cfg(test) only)  
**Source:** [`soroban/src/sidecar_state.rs`](../src/sidecar_state.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `NotAuthorized` | _(no doc comment on the variant; name reads as: not authorized)_ |
| 2 | `AlreadyInitialized` | _(no doc comment on the variant; name reads as: already initialized)_ |
| 3 | `EntityNotFound` | _(no doc comment on the variant; name reads as: entity not found)_ |
| 4 | `SidecarNotFound` | _(no doc comment on the variant; name reads as: sidecar not found)_ |
| 5 | `MaxEntriesExceeded` | _(no doc comment on the variant; name reads as: max entries exceeded)_ |
| 6 | `ConsistencyCheckFailed` | _(no doc comment on the variant; name reads as: consistency check failed)_ |
| 7 | `InvalidReference` | _(no doc comment on the variant; name reads as: invalid reference)_ |

## `Error`

**Contract:** BridgeReserveVerifier — Part 5 — Unreferenced file (not compiled anywhere)  
**Source:** [`soroban/src/bridge_reserve_verifier.rs`](../src/bridge_reserve_verifier.rs)

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `NotInitialized` | _(no doc comment on the variant; name reads as: not initialized)_ |
| 2 | `AlreadyInitialized` | _(no doc comment on the variant; name reads as: already initialized)_ |
| 3 | `Unauthorized` | _(no doc comment on the variant; name reads as: unauthorized)_ |
| 4 | `BridgeNotFound` | _(no doc comment on the variant; name reads as: bridge not found)_ |
| 5 | `BridgeAlreadyRegistered` | _(no doc comment on the variant; name reads as: bridge already registered)_ |
| 6 | `OperatorNotFound` | _(no doc comment on the variant; name reads as: operator not found)_ |
| 7 | `CommitmentNotFound` | _(no doc comment on the variant; name reads as: commitment not found)_ |
| 8 | `InvalidProof` | _(no doc comment on the variant; name reads as: invalid proof)_ |
| 9 | `ChallengePeriodActive` | _(no doc comment on the variant; name reads as: challenge period active)_ |
| 10 | `ChallengePeriodExpired` | _(no doc comment on the variant; name reads as: challenge period expired)_ |
| 11 | `InsufficientStake` | _(no doc comment on the variant; name reads as: insufficient stake)_ |
| 12 | `OperatorInactive` | _(no doc comment on the variant; name reads as: operator inactive)_ |
| 13 | `InvalidInput` | _(no doc comment on the variant; name reads as: invalid input)_ |
| 14 | `NotChallengeable` | _(no doc comment on the variant; name reads as: not challengeable)_ |
| 15 | `NotResolvable` | _(no doc comment on the variant; name reads as: not resolvable)_ |

## Cross-contract uniqueness check

Verified programmatically (by walking each `#[contracterror]` enum's declared discriminants) that:

1. Every contract's own error codes are contiguous, starting at `1`, with no gaps or duplicate values.
2. No two `#[contracterror]` enums in the workspace share the same Rust type name, so there is no identifier collision even though several reuse the same *numeric* range (every enum here starts at 1).

| Contract | Enum | Codes | Range check |
| --- | --- | --- | --- |
| CrossChainRelayContract | `RelayError` | 17 | contiguous 1..17 |
| EmergencyFundRecovery | `RecoveryError` | 14 | contiguous 1..14 |
| AssetDeprecationContract | `DeprecationError` | 7 | contiguous 1..7 |
| AssetRegistryContract | `RegistryError` | 22 | contiguous 1..22 |
| BatchQueryContract | `BatchQueryError` | 4 | contiguous 1..4 |
| RateLimiterContract | `RateLimitError` | 16 | contiguous 1..16 |
| SidecarStateContract | `SidecarError` | 7 | contiguous 1..7 |
| BridgeReserveVerifier | `Error` | 15 | contiguous 1..15 |

## Non-numeric result errors

Two modules define a `#[contracttype]` enum named `MigrationError` (not `#[contracterror]`, so these are ordinary Soroban data types returned inside `Result<T, MigrationError>` — they do not carry a raw numeric error code the way the enums above do, and are decoded by variant name rather than by integer). They are **different types in different modules** (no compile-time collision), but share a name, which is worth knowing if you're importing both:

| Module | Variants |
| --- | --- |
| `migration.rs` | `AlreadyAtVersion`, `VersionDowngradeNotAllowed`, `UnauthorizedMigrator`, `ValidationFailed`, `RollbackNotAvailable` |
| `version_migration_helper.rs` | `AlreadyAtVersion`, `VersionDowngradeNotAllowed`, `UnauthorizedMigrator`, `ValidationFailed`, `RollbackNotAvailable`, `InvalidStateSnapshot`, `SnapshotExpired`, `NoValidationResults`, `MigrationInProgress`, `StateIntegrityCheckFailed` |

`version_migration_helper::MigrationError` is a superset of `migration::MigrationError` plus five additional variants (`InvalidStateSnapshot`, `SnapshotExpired`, `NoValidationResults`, `MigrationInProgress`, `StateIntegrityCheckFailed`) — see [`version_migration_helper` in API_REFERENCE.md](./API_REFERENCE.md#version_migration_helper--enhanced-migration-helper) for the fuller migration system these variants belong to.
