# Trusted Source Registry Implementation

## Overview

This document describes the implementation of the Trusted Source Registry feature for the Bridge Watch Soroban contract. This feature provides an additional layer of access control for contract submissions by maintaining a registry of trusted external sources.

## Implementation Summary

### Files Created

1. **`src/source_trust.rs`** - Core module implementing the trusted source registry
2. **`tests/source_trust.test.rs`** - Comprehensive test suite
3. **`docs/TRUSTED_SOURCE_REGISTRY.md`** - User-facing documentation

### Files Modified

1. **`src/lib.rs`** - Integrated the source trust module and added public API methods
   - Added module declaration
   - Added storage key constants
   - Added 6 public contract methods
   - Updated `submit_health()` to gate by source trust
   - Updated `submit_price()` to gate by source trust

## Features Implemented

### ✅ Register Trusted Sources

- Admin-only operation
- Stores source address, name, and audit metadata
- Supports reactivation of previously revoked sources
- Emits `SourceRegisteredEvent`

### ✅ Revoke Sources

- Admin-only operation
- Marks source as inactive while preserving audit trail
- Prevents double revocation
- Emits `SourceRevokedEvent`

### ✅ Query Source Status

- `is_trusted_source()` - Quick boolean check
- `get_trusted_source()` - Detailed source information
- `get_all_trusted_sources()` - List all sources (active and revoked)
- `get_active_trusted_sources()` - List only active sources

### ✅ Admin-Only Writes

- All registration and revocation operations require admin or super admin permissions
- Uses existing ACL system with `ManageConfig` permission
- Contract admin has inherent access

### ✅ Event Emission

- `SourceRegisteredEvent` - Emitted on registration
- `SourceRevokedEvent` - Emitted on revocation
- Events include actor, timestamp, and relevant details

### ✅ Audit-Friendly Records

- Complete audit trail for each source:
  - Who registered it and when
  - Who revoked it and when (if revoked)
  - Current active status
  - Historical changes preserved
- All records stored persistently

### ✅ Submission Gating

- `submit_health()` checks source trust when sources are registered
- `submit_price()` checks source trust when sources are registered
- Opt-in behavior: enforcement only activates when sources are registered
- Backward compatible with existing deployments

## Architecture

### Data Structures

```rust
pub struct TrustedSource {
    pub source_address: Address,
    pub name: String,
    pub registered_by: Address,
    pub registered_at: u64,
    pub is_active: bool,
    pub revoked_by: Option<Address>,
    pub revoked_at: Option<u64>,
}

pub struct SourceInfo {
    pub source_address: Address,
    pub name: String,
    pub is_active: bool,
    pub registered_at: u64,
}
```

### Storage Layout

```
SourceTrustKey::Source(Address) -> TrustedSource
SourceTrustKey::AllSources -> Vec<Address>
```

### Trust Enforcement Flow

```
Submission Request
    ↓
Check Role Permissions (existing)
    ↓
Check if any sources registered?
    ├─ No → Allow (backward compatible)
    └─ Yes → Check if caller is trusted source
        ├─ Yes → Allow
        └─ No → Reject
```

## Testing

### Test Coverage

The test suite includes 20+ tests covering:

1. **Basic Operations**
   - Register single source
   - Register multiple sources
   - Revoke source
   - Reactivate revoked source

2. **Query Operations**
   - Check trust status
   - Get source details
   - List all sources
   - List active sources only

3. **Edge Cases**
   - Unregistered sources
   - Double revocation
   - Empty names
   - Multiple registrations

4. **Audit Trail**
   - Registration metadata
   - Revocation metadata
   - Timestamp tracking

5. **Integration Tests**
   - Submission gating with health scores
   - Submission gating with prices
   - Multiple trusted sources
   - Admin bypass
   - Revoked source rejection

### Running Tests

```bash
# Run all tests
cargo test --package bridge-watch-soroban

# Run only source trust tests
cargo test --package bridge-watch-soroban --test source_trust

# Run with output
cargo test --package bridge-watch-soroban --test source_trust -- --nocapture
```

## Security Considerations

### Defense in Depth

The implementation provides multiple security layers:

1. **Authentication**: Caller must authenticate via `require_auth()`
2. **Authorization**: Caller must have appropriate role (RBAC)
3. **Trust**: Caller must be a registered trusted source (when enabled)
4. **Asset State**: Asset must be active and not paused

### Audit Trail

Every action is recorded with:

- Actor address (who performed the action)
- Timestamp (when it occurred)
- Current state (active/revoked)
- Historical changes (preserved for audit)

### Opt-In Design

The trust enforcement is opt-in to ensure:

- Backward compatibility with existing deployments
- Gradual rollout capability
- No breaking changes to existing functionality

## Usage Examples

### Initial Setup

```rust
// 1. Initialize contract
contract.initialize(env, admin_address);

// 2. Register trusted sources
contract.register_trusted_source(
    env,
    admin_address,
    oracle_address,
    "CoinGecko Oracle".into(),
);

// 3. Grant submission permissions
contract.grant_role(
    env,
    admin_address,
    oracle_address,
    AdminRole::PriceSubmitter,
);

// 4. Source can now submit
contract.submit_price(
    env,
    oracle_address,
    "USDC".into(),
    1_000_000,
    "coingecko".into(),
);
```

### Source Rotation

```rust
// Revoke old source
contract.revoke_trusted_source(env, admin_address, old_oracle);

// Register new source
contract.register_trusted_source(
    env,
    admin_address,
    new_oracle,
    "Updated Oracle v2".into(),
);

// Grant permissions
contract.grant_role(
    env,
    admin_address,
    new_oracle,
    AdminRole::PriceSubmitter,
);
```

### Monitoring

```rust
// Check if source is trusted
let is_trusted = contract.is_trusted_source(env, oracle_address);

// Get detailed info
if let Some(source) = contract.get_trusted_source(env, oracle_address) {
    log!("Source: {}, Active: {}", source.name, source.is_active);
}

// List all active sources
let active = contract.get_active_trusted_sources(env);
log!("Active sources: {}", active.len());
```

## Migration Guide

### For Existing Deployments

1. **Deploy updated contract** with trusted source registry
2. **No immediate changes required** - feature is opt-in
3. **Register sources gradually** as needed
4. **Monitor submissions** to ensure trusted sources work correctly
5. **Revoke old sources** when rotating to new ones

### For New Deployments

1. Initialize contract
2. Register all trusted sources upfront
3. Grant roles to sources
4. Begin normal operations

## Future Enhancements

Potential improvements for future versions:

1. **Source Expiration**: Automatic revocation after time period
2. **Source Quotas**: Rate limiting per source
3. **Source Reputation**: Track submission quality
4. **Multi-Sig Registration**: Require multiple admins
5. **Source Categories**: Different trust levels for different data types
6. **Batch Operations**: Register/revoke multiple sources at once

## Commit Message

```
feat: implement trusted source registry

Add trusted source registry for contract submissions and score updates.

Features:
- Register/revoke trusted sources (admin-only)
- Query source status and details
- Gate submissions by source trust
- Event emission for all actions
- Complete audit trail
- Opt-in enforcement (backward compatible)

The registry provides an additional security layer beyond role-based
access control. When sources are registered, all submissions must come
from trusted sources. The feature is opt-in to maintain backward
compatibility with existing deployments.

Closes #[issue-number]
```

## Documentation

- **User Guide**: `docs/TRUSTED_SOURCE_REGISTRY.md`
- **API Reference**: Inline documentation in `src/source_trust.rs`
- **Test Suite**: `tests/source_trust.test.rs`
- **This Document**: Implementation details and architecture

## Checklist

- [x] Add source registry storage
- [x] Create register function
- [x] Create revoke function
- [x] Gate submissions by source trust
- [x] Add comprehensive tests
- [x] Document trust model
- [x] Event emission
- [x] Audit trail
- [x] Admin-only writes
- [x] Query functions
- [x] Integration tests
- [x] User documentation
