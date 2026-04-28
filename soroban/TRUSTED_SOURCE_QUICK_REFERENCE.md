# Trusted Source Registry - Quick Reference

## Quick Start

### Register a Source

```rust
contract.register_trusted_source(
    env,
    admin_address,
    oracle_address,
    "Oracle Name".into(),
);
```

### Revoke a Source

```rust
contract.revoke_trusted_source(env, admin_address, oracle_address);
```

### Check if Trusted

```rust
let is_trusted = contract.is_trusted_source(env, oracle_address);
```

## API Methods

| Method                         | Description                     | Admin Required |
| ------------------------------ | ------------------------------- | -------------- |
| `register_trusted_source()`    | Register or reactivate a source | ✅             |
| `revoke_trusted_source()`      | Revoke a source                 | ✅             |
| `is_trusted_source()`          | Check if source is trusted      | ❌             |
| `get_trusted_source()`         | Get source details              | ❌             |
| `get_all_trusted_sources()`    | List all sources                | ❌             |
| `get_active_trusted_sources()` | List active sources only        | ❌             |

## Events

| Event                   | Topic     | When Emitted      |
| ----------------------- | --------- | ----------------- |
| `SourceRegisteredEvent` | `src_reg` | Source registered |
| `SourceRevokedEvent`    | `src_rev` | Source revoked    |

## Data Structures

### TrustedSource

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

### SourceInfo

```rust
pub struct SourceInfo {
    pub source_address: Address,
    pub name: String,
    pub is_active: bool,
    pub registered_at: u64,
}
```

## Common Patterns

### Setup New Source

```rust
// 1. Register source
contract.register_trusted_source(
    env,
    admin,
    oracle,
    "Oracle".into(),
);

// 2. Grant role
contract.grant_role(
    env,
    admin,
    oracle,
    AdminRole::PriceSubmitter,
);

// 3. Source can submit
contract.submit_price(
    env,
    oracle,
    "USDC".into(),
    1_000_000,
    "oracle".into(),
);
```

### Rotate Source

```rust
// 1. Revoke old
contract.revoke_trusted_source(env, admin, old_oracle);

// 2. Register new
contract.register_trusted_source(env, admin, new_oracle, "New Oracle".into());

// 3. Grant role
contract.grant_role(env, admin, new_oracle, AdminRole::PriceSubmitter);
```

### Monitor Sources

```rust
// Get all active sources
let active = contract.get_active_trusted_sources(env);
for source in active.iter() {
    log!("Active: {}", source.name);
}

// Check specific source
if let Some(source) = contract.get_trusted_source(env, oracle) {
    if !source.is_active {
        log!("Warning: {} is revoked", source.name);
    }
}
```

## Trust Enforcement

### When Enabled

Trust enforcement activates when **any** source is registered:

```rust
// Before: No sources registered
contract.submit_health(caller, ...); // ✅ Works (role-based only)

// After: First source registered
contract.register_trusted_source(env, admin, oracle, "Oracle".into());
contract.submit_health(caller, ...); // ❌ Fails if caller not trusted
contract.submit_health(oracle, ...); // ✅ Works (oracle is trusted)
```

### Bypass

Admin always bypasses trust checks:

```rust
contract.register_trusted_source(env, admin, oracle, "Oracle".into());
contract.submit_health(admin, ...); // ✅ Always works (admin bypass)
```

## Error Messages

| Error                                                  | Cause                               |
| ------------------------------------------------------ | ----------------------------------- |
| `"source name cannot be empty"`                        | Empty name in registration          |
| `"source not registered"`                              | Revoking unregistered source        |
| `"source already revoked"`                             | Double revocation                   |
| `"caller is not a trusted source"`                     | Untrusted submission attempt        |
| `"unauthorized: caller lacks the required permission"` | Non-admin trying to register/revoke |

## Testing

### Run Tests

```bash
# All tests
cargo test --package bridge-watch-soroban

# Source trust only
cargo test --package bridge-watch-soroban --test source_trust

# Specific test
cargo test --package bridge-watch-soroban --test source_trust test_register_trusted_source
```

### Test Template

```rust
#[test]
fn test_my_scenario() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, BridgeWatchContract);
    let client = BridgeWatchContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let source = Address::generate(&env);

    client.initialize(&admin);

    // Your test here
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Test"));
    assert!(client.is_trusted_source(&source));
}
```

## Storage Keys

```rust
SourceTrustKey::Source(Address) -> TrustedSource
SourceTrustKey::AllSources -> Vec<Address>
```

## Permissions Required

| Operation | Permission     | Alternative    |
| --------- | -------------- | -------------- |
| Register  | `ManageConfig` | Contract admin |
| Revoke    | `ManageConfig` | Contract admin |
| Query     | None           | Public         |

## Best Practices

1. ✅ Register sources before granting roles
2. ✅ Use descriptive names for audit trail
3. ✅ Monitor events in production
4. ✅ Regular source reviews
5. ✅ Revoke before rotating
6. ✅ Test on testnet first

## Common Mistakes

❌ **Granting role before registering source**

```rust
// Wrong order
contract.grant_role(env, admin, oracle, AdminRole::PriceSubmitter);
contract.register_trusted_source(env, admin, oracle, "Oracle".into());
```

✅ **Register source first**

```rust
// Correct order
contract.register_trusted_source(env, admin, oracle, "Oracle".into());
contract.grant_role(env, admin, oracle, AdminRole::PriceSubmitter);
```

❌ **Forgetting to check trust status**

```rust
// No check
contract.submit_price(oracle, ...); // May fail
```

✅ **Check before submission**

```rust
// Check first
if contract.is_trusted_source(env, oracle) {
    contract.submit_price(oracle, ...);
}
```

## Documentation Links

- **User Guide**: `docs/TRUSTED_SOURCE_REGISTRY.md`
- **Implementation**: `TRUSTED_SOURCE_IMPLEMENTATION.md`
- **Tests**: `tests/source_trust.test.rs`
- **Source Code**: `src/source_trust.rs`

## Support

For questions or issues:

1. Check the full documentation
2. Review test cases for examples
3. Open an issue on GitHub
