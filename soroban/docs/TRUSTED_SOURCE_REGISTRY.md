# Trusted Source Registry

## Overview

The Trusted Source Registry is a security feature that controls which external addresses are authorized to submit contract data (health scores, price updates, etc.) to the Bridge Watch contract. This provides an additional layer of access control beyond role-based permissions.

## Trust Model

### Hierarchy

```
Admin / SuperAdmin
  └─ Can register new trusted sources
  └─ Can revoke existing sources
  └─ Can query source status

Trusted Source (Address)
  └─ Can submit health scores
  └─ Can submit price updates
  └─ Can submit other contract data

Untrusted Source
  └─ Submissions are rejected
```

### How It Works

1. **Registration Phase**: Admins register trusted source addresses with descriptive names
2. **Enforcement Phase**: When sources are registered, all submissions are gated by trust status
3. **Revocation Phase**: Admins can revoke sources, preventing further submissions
4. **Audit Phase**: All actions are logged with timestamps and actor information

### Opt-In Behavior

The trust enforcement is **opt-in**:

- If no trusted sources are registered, submissions work as before (role-based only)
- Once the first source is registered, trust enforcement activates
- All subsequent submissions must come from trusted sources

This ensures backward compatibility while providing enhanced security when needed.

## API Reference

### Register Trusted Source

```rust
pub fn register_trusted_source(
    env: Env,
    caller: Address,
    source_address: Address,
    name: String,
)
```

**Description**: Register a new trusted source or reactivate a previously revoked one.

**Parameters**:

- `caller`: Admin performing the registration (must have `ManageConfig` permission)
- `source_address`: The address to register as a trusted source
- `name`: Human-readable name/description for the source

**Panics**:

- If `caller` is not an admin or super admin
- If `name` is empty

**Events**: Emits `SourceRegisteredEvent`

**Example**:

```rust
contract.register_trusted_source(
    env,
    admin_address,
    oracle_address,
    "CoinGecko Price Oracle".into(),
);
```

### Revoke Trusted Source

```rust
pub fn revoke_trusted_source(
    env: Env,
    caller: Address,
    source_address: Address,
)
```

**Description**: Revoke a trusted source, preventing it from making further submissions.

**Parameters**:

- `caller`: Admin performing the revocation (must have `ManageConfig` permission)
- `source_address`: The address to revoke

**Panics**:

- If `caller` is not an admin or super admin
- If `source_address` is not registered
- If `source_address` is already revoked

**Events**: Emits `SourceRevokedEvent`

**Example**:

```rust
contract.revoke_trusted_source(env, admin_address, oracle_address);
```

### Check Trust Status

```rust
pub fn is_trusted_source(env: Env, source_address: Address) -> bool
```

**Description**: Check if an address is currently a trusted source.

**Parameters**:

- `source_address`: The address to check

**Returns**: `true` if the address is registered and active, `false` otherwise

**Example**:

```rust
let is_trusted = contract.is_trusted_source(env, oracle_address);
if is_trusted {
    // Allow submission
}
```

### Get Source Details

```rust
pub fn get_trusted_source(
    env: Env,
    source_address: Address,
) -> Option<TrustedSource>
```

**Description**: Get detailed information about a trusted source.

**Parameters**:

- `source_address`: The address to query

**Returns**: `Some(TrustedSource)` if registered, `None` otherwise

**TrustedSource Structure**:

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
```

### List All Sources

```rust
pub fn get_all_trusted_sources(env: Env) -> Vec<SourceInfo>
```

**Description**: Get a list of all registered trusted sources (active and revoked).

**Returns**: Vector of `SourceInfo` records

**SourceInfo Structure**:

```rust
pub struct SourceInfo {
    pub source_address: Address,
    pub name: String,
    pub is_active: bool,
    pub registered_at: u64,
}
```

### List Active Sources

```rust
pub fn get_active_trusted_sources(env: Env) -> Vec<SourceInfo>
```

**Description**: Get a list of only active trusted sources.

**Returns**: Vector of `SourceInfo` records for active sources only

## Events

### SourceRegisteredEvent

Emitted when a trusted source is registered.

```rust
pub struct SourceRegisteredEvent {
    pub source_address: Address,
    pub name: String,
    pub registered_by: Address,
    pub timestamp: u64,
}
```

**Topic**: `src_reg`

### SourceRevokedEvent

Emitted when a trusted source is revoked.

```rust
pub struct SourceRevokedEvent {
    pub source_address: Address,
    pub revoked_by: Address,
    pub timestamp: u64,
}
```

**Topic**: `src_rev`

## Usage Patterns

### Initial Setup

```rust
// 1. Initialize contract
contract.initialize(env, admin_address);

// 2. Register trusted sources
contract.register_trusted_source(
    env,
    admin_address,
    coingecko_oracle,
    "CoinGecko Price Oracle".into(),
);

contract.register_trusted_source(
    env,
    admin_address,
    chainlink_oracle,
    "Chainlink Price Feed".into(),
);

// 3. Grant submission permissions to sources
contract.grant_role(
    env,
    admin_address,
    coingecko_oracle,
    AdminRole::PriceSubmitter,
);
```

### Rotating Sources

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

// Grant permissions to new source
contract.grant_role(
    env,
    admin_address,
    new_oracle,
    AdminRole::PriceSubmitter,
);
```

### Monitoring Sources

```rust
// Get all sources for audit
let all_sources = contract.get_all_trusted_sources(env);
for source in all_sources.iter() {
    log!(
        "Source: {}, Active: {}, Registered: {}",
        source.name,
        source.is_active,
        source.registered_at
    );
}

// Check specific source
if let Some(source) = contract.get_trusted_source(env, oracle_address) {
    if !source.is_active {
        log!("Warning: Source {} was revoked", source.name);
    }
}
```

## Security Considerations

### Defense in Depth

The trusted source registry provides an additional security layer:

1. **Role-Based Access Control (RBAC)**: First layer - checks if caller has the right role
2. **Trusted Source Registry**: Second layer - checks if caller is a registered trusted source
3. **Asset-Level Controls**: Third layer - checks if asset is active and not paused

All three layers must pass for a submission to succeed.

### Audit Trail

Every action is recorded:

- Who registered/revoked the source
- When the action occurred
- Current status of each source
- Historical changes preserved

This provides a complete audit trail for compliance and security reviews.

### Best Practices

1. **Register sources before granting roles**: Ensure sources are trusted before giving them permissions
2. **Use descriptive names**: Make it easy to identify sources in audit logs
3. **Regular reviews**: Periodically review active sources and revoke unused ones
4. **Rotate sources**: When updating oracles or data providers, revoke old and register new
5. **Monitor events**: Watch for `SourceRegisteredEvent` and `SourceRevokedEvent` in production

### Emergency Response

If a source is compromised:

```rust
// 1. Immediately revoke the source
contract.revoke_trusted_source(env, admin_address, compromised_source);

// 2. Revoke its roles
contract.revoke_role(
    env,
    admin_address,
    compromised_source,
    AdminRole::PriceSubmitter,
);

// 3. Register replacement source
contract.register_trusted_source(
    env,
    admin_address,
    new_source,
    "Replacement Oracle".into(),
);
```

## Testing

Comprehensive tests are provided in `tests/source_trust.test.rs`:

- Registration and revocation
- Multiple sources
- Reactivation of revoked sources
- Active vs. all sources queries
- Audit trail verification
- Error cases (empty names, double revocation, etc.)

Run tests:

```bash
cargo test --package bridge-watch-soroban --test source_trust
```

## Migration Guide

### Existing Deployments

For contracts already deployed without trusted source registry:

1. **No immediate action required**: The feature is opt-in
2. **To enable**: Register your first trusted source
3. **Gradual rollout**: Register sources one at a time
4. **Verify**: Test submissions from registered sources
5. **Enforce**: Once all sources are registered, trust is automatically enforced

### New Deployments

For new contract deployments:

1. Initialize contract
2. Register all trusted sources
3. Grant roles to sources
4. Begin operations

## Future Enhancements

Potential future improvements:

- **Source expiration**: Automatic revocation after a time period
- **Source quotas**: Limit submission rates per source
- **Source reputation**: Track submission quality and accuracy
- **Multi-signature registration**: Require multiple admins to register sources
- **Source categories**: Different trust levels for different data types

## References

- [ACL Module Documentation](../src/acl.rs)
- [Bridge Watch Contract](../src/lib.rs)
- [Test Suite](../tests/source_trust.test.rs)
