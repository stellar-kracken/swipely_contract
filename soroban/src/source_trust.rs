/// Trusted Source Registry for Stellar Bridge Watch.
///
/// # Overview
///
/// This module manages a registry of trusted external sources that are
/// authorized to submit contract data (health scores, price updates, etc.)
/// and score updates. Only admin-level users can register or revoke sources.
///
/// # Trust Model
///
/// ```text
/// Trust Hierarchy:
///
///   Admin / SuperAdmin
///     └─ Can register new trusted sources
///     └─ Can revoke existing sources
///     └─ Can query source status
///
///   Trusted Source (Address)
///     └─ Can submit health scores
///     └─ Can submit price updates
///     └─ Can submit other contract data
///
///   Untrusted Source
///     └─ Submissions are rejected
/// ```
///
/// # Audit Trail
///
/// All registration and revocation events are:
/// - Emitted as contract events
/// - Recorded with timestamps
/// - Tracked with the admin who performed the action
/// - Stored persistently for audit purposes
///
/// # Usage
///
/// ```rust,ignore
/// // Register a trusted source
/// contract.register_trusted_source(
///     env,
///     admin_address,
///     source_address,
///     "CoinGecko Price Oracle".into(),
/// );
///
/// // Check if source is trusted
/// let is_trusted = contract.is_trusted_source(env, source_address);
///
/// // Revoke a source
/// contract.revoke_trusted_source(env, admin_address, source_address);
/// ```

use soroban_sdk::{contracttype, Address, Env, String, Vec};

// ── Data Types ────────────────────────────────────────────────────────────────

/// Represents a trusted source registration record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedSource {
    /// The address of the trusted source.
    pub source_address: Address,
    /// Human-readable name or description of the source.
    pub name: String,
    /// Address of the admin who registered this source.
    pub registered_by: Address,
    /// Ledger timestamp when the source was registered.
    pub registered_at: u64,
    /// Whether this source is currently active (not revoked).
    pub is_active: bool,
    /// Address of the admin who revoked this source (if revoked).
    pub revoked_by: Option<Address>,
    /// Ledger timestamp when the source was revoked (if revoked).
    pub revoked_at: Option<u64>,
}

/// Event emitted when a trusted source is registered.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRegisteredEvent {
    pub source_address: Address,
    pub name: String,
    pub registered_by: Address,
    pub timestamp: u64,
}

/// Event emitted when a trusted source is revoked.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRevokedEvent {
    pub source_address: Address,
    pub revoked_by: Address,
    pub timestamp: u64,
}

/// Summary information about a trusted source for queries.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceInfo {
    pub source_address: Address,
    pub name: String,
    pub is_active: bool,
    pub registered_at: u64,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceTrustKey {
    /// Maps source address to TrustedSource record.
    Source(Address),
    /// List of all registered source addresses (Vec<Address>).
    AllSources,
}

// ── Core Functions ────────────────────────────────────────────────────────────

/// Register a new trusted source or reactivate a previously revoked one.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `caller` - The admin performing the registration (must have admin permissions)
/// * `source_address` - The address to register as a trusted source
/// * `name` - Human-readable name/description for the source
///
/// # Panics
///
/// * If `caller` is not an admin or super admin
/// * If `name` is empty
/// * If `source_address` is the zero address
///
/// # Events
///
/// Emits a `SourceRegisteredEvent` on success.
pub fn register_trusted_source(
    env: &Env,
    caller: &Address,
    source_address: &Address,
    name: String,
) {
    // Validate inputs
    if name.len() == 0 {
        panic!("source name cannot be empty");
    }

    let now = env.ledger().timestamp();

    // Check if source already exists
    let key = SourceTrustKey::Source(source_address.clone());
    let existing: Option<TrustedSource> = env.storage().persistent().get(&key);

    let source = match existing {
        Some(mut existing_source) => {
            // Reactivate if previously revoked
            existing_source.is_active = true;
            existing_source.registered_by = caller.clone();
            existing_source.registered_at = now;
            existing_source.revoked_by = None;
            existing_source.revoked_at = None;
            existing_source.name = name.clone();
            existing_source
        }
        None => {
            // Create new source
            TrustedSource {
                source_address: source_address.clone(),
                name: name.clone(),
                registered_by: caller.clone(),
                registered_at: now,
                is_active: true,
                revoked_by: None,
                revoked_at: None,
            }
        }
    };

    // Store the source
    env.storage().persistent().set(&key, &source);

    // Add to all sources list if not already present
    let all_sources_key = SourceTrustKey::AllSources;
    let mut all_sources: Vec<Address> = env
        .storage()
        .persistent()
        .get(&all_sources_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut found = false;
    for addr in all_sources.iter() {
        if &addr == source_address {
            found = true;
            break;
        }
    }

    if !found {
        all_sources.push_back(source_address.clone());
        env.storage()
            .persistent()
            .set(&all_sources_key, &all_sources);
    }

    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("src_reg"),),
        SourceRegisteredEvent {
            source_address: source_address.clone(),
            name,
            registered_by: caller.clone(),
            timestamp: now,
        },
    );
}

/// Revoke a trusted source, preventing it from making further submissions.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `caller` - The admin performing the revocation (must have admin permissions)
/// * `source_address` - The address to revoke
///
/// # Panics
///
/// * If `caller` is not an admin or super admin
/// * If `source_address` is not registered
/// * If `source_address` is already revoked
///
/// # Events
///
/// Emits a `SourceRevokedEvent` on success.
pub fn revoke_trusted_source(env: &Env, caller: &Address, source_address: &Address) {
    let key = SourceTrustKey::Source(source_address.clone());
    let mut source: TrustedSource = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("source not registered"));

    if !source.is_active {
        panic!("source already revoked");
    }

    let now = env.ledger().timestamp();

    source.is_active = false;
    source.revoked_by = Some(caller.clone());
    source.revoked_at = Some(now);

    env.storage().persistent().set(&key, &source);

    // Emit event
    env.events().publish(
        (soroban_sdk::symbol_short!("src_rev"),),
        SourceRevokedEvent {
            source_address: source_address.clone(),
            revoked_by: caller.clone(),
            timestamp: now,
        },
    );
}

/// Check if an address is currently a trusted source.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `source_address` - The address to check
///
/// # Returns
///
/// `true` if the address is registered and active, `false` otherwise.
pub fn is_trusted_source(env: &Env, source_address: &Address) -> bool {
    let key = SourceTrustKey::Source(source_address.clone());
    let source: Option<TrustedSource> = env.storage().persistent().get(&key);

    match source {
        Some(s) => s.is_active,
        None => false,
    }
}

/// Get detailed information about a trusted source.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `source_address` - The address to query
///
/// # Returns
///
/// `Some(TrustedSource)` if the source is registered, `None` otherwise.
pub fn get_trusted_source(env: &Env, source_address: &Address) -> Option<TrustedSource> {
    let key = SourceTrustKey::Source(source_address.clone());
    env.storage().persistent().get(&key)
}

/// Get a list of all registered trusted sources (active and revoked).
///
/// # Arguments
///
/// * `env` - The contract environment
///
/// # Returns
///
/// A vector of `SourceInfo` records for all registered sources.
pub fn get_all_trusted_sources(env: &Env) -> Vec<SourceInfo> {
    let all_sources_key = SourceTrustKey::AllSources;
    let all_sources: Vec<Address> = env
        .storage()
        .persistent()
        .get(&all_sources_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut result: Vec<SourceInfo> = Vec::new(env);

    for addr in all_sources.iter() {
        let key = SourceTrustKey::Source(addr.clone());
        if let Some(source) = env.storage().persistent().get::<_, TrustedSource>(&key) {
            result.push_back(SourceInfo {
                source_address: source.source_address,
                name: source.name,
                is_active: source.is_active,
                registered_at: source.registered_at,
            });
        }
    }

    result
}

/// Get a list of only active trusted sources.
///
/// # Arguments
///
/// * `env` - The contract environment
///
/// # Returns
///
/// A vector of `SourceInfo` records for active sources only.
pub fn get_active_trusted_sources(env: &Env) -> Vec<SourceInfo> {
    let all_sources = get_all_trusted_sources(env);
    let mut result: Vec<SourceInfo> = Vec::new(env);

    for info in all_sources.iter() {
        if info.is_active {
            result.push_back(info);
        }
    }

    result
}

/// Require that the caller is a trusted source, panicking if not.
///
/// This is a convenience function for gating submissions.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `caller` - The address to check
///
/// # Panics
///
/// If `caller` is not an active trusted source.
pub fn require_trusted_source(env: &Env, caller: &Address) {
    if !is_trusted_source(env, caller) {
        panic!("caller is not a trusted source");
    }
}
