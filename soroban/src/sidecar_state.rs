//! # Sidecar State Store
//!
//! Store auxiliary state off-contract while maintaining referential integrity
//! with on-chain data. Provides consistency checks and query support.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of sidecar entries per entity
pub const MAX_SIDECAR_ENTRIES: u32 = 100;

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SidecarError {
    NotAuthorized = 1,
    AlreadyInitialized = 2,
    EntityNotFound = 3,
    SidecarNotFound = 4,
    MaxEntriesExceeded = 5,
    ConsistencyCheckFailed = 6,
    InvalidReference = 7,
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Sidecar state entry with referential integrity
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SidecarEntry {
    /// Unique identifier for this sidecar entry
    pub entry_id: String,
    /// Reference to on-chain entity
    pub entity_ref: String,
    /// Hash of the referenced entity state (for consistency)
    pub entity_hash: String,
    /// Auxiliary data stored off-contract
    pub data: String,
    /// Entry metadata
    pub metadata: String,
    /// Timestamp when created
    pub created_at: u64,
    /// Timestamp when last updated
    pub updated_at: u64,
    /// Creator address
    pub created_by: Address,
}

/// Consistency check result
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsistencyCheck {
    /// Whether the check passed
    pub is_consistent: bool,
    /// Expected hash
    pub expected_hash: String,
    /// Actual hash
    pub actual_hash: String,
    /// Timestamp of check
    pub checked_at: u64,
}

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    /// Contract admin address
    Admin,
    /// Sidecar entries for an entity (Vec<SidecarEntry>)
    SidecarEntries(String),
    /// Index of all entity references (Vec<String>)
    EntityIndex,
    /// Mapping of entry_id to entity_ref
    EntryToEntity(String),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct SidecarStateContract;

#[contractimpl]
impl SidecarStateContract {
    /// Initialize the contract with an admin address
    pub fn initialize(env: Env, admin: Address) -> Result<(), SidecarError> {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(SidecarError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);

        let empty: Vec<String> = Vec::new(&env);
        env.storage().instance().set(&DataKey::EntityIndex, &empty);

        Ok(())
    }

    /// Store a new sidecar entry linked to an on-chain entity
    pub fn store_sidecar(
        env: Env,
        caller: Address,
        entry_id: String,
        entity_ref: String,
        entity_hash: String,
        data: String,
        metadata: String,
    ) -> Result<(), SidecarError> {
        caller.require_auth();

        let mut entries: Vec<SidecarEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::SidecarEntries(entity_ref.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        if entries.len() >= MAX_SIDECAR_ENTRIES {
            return Err(SidecarError::MaxEntriesExceeded);
        }

        let now = env.ledger().timestamp();
        let entry = SidecarEntry {
            entry_id: entry_id.clone(),
            entity_ref: entity_ref.clone(),
            entity_hash,
            data,
            metadata,
            created_at: now,
            updated_at: now,
            created_by: caller,
        };

        entries.push_back(entry);
        env.storage()
            .persistent()
            .set(&DataKey::SidecarEntries(entity_ref.clone()), &entries);

        // Store entry to entity mapping
        env.storage()
            .persistent()
            .set(&DataKey::EntryToEntity(entry_id.clone()), &entity_ref);

        // Update entity index
        Self::add_to_entity_index(&env, &entity_ref);

        env.events()
            .publish((symbol_short!("sc_store"), entry_id), entity_ref);

        Ok(())
    }

    /// Update an existing sidecar entry
    pub fn update_sidecar(
        env: Env,
        caller: Address,
        entry_id: String,
        new_entity_hash: String,
        new_data: String,
        new_metadata: String,
    ) -> Result<(), SidecarError> {
        caller.require_auth();

        let entity_ref = Self::get_entity_for_entry(&env, &entry_id)?;
        let mut entries: Vec<SidecarEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::SidecarEntries(entity_ref.clone()))
            .ok_or(SidecarError::SidecarNotFound)?;

        let mut found = false;
        let now = env.ledger().timestamp();

        for i in 0..entries.len() {
            if let Some(mut entry) = entries.get(i) {
                if entry.entry_id == entry_id {
                    entry.entity_hash = new_entity_hash;
                    entry.data = new_data;
                    entry.metadata = new_metadata;
                    entry.updated_at = now;
                    entries.set(i, entry);
                    found = true;
                    break;
                }
            }
        }

        if !found {
            return Err(SidecarError::SidecarNotFound);
        }

        env.storage()
            .persistent()
            .set(&DataKey::SidecarEntries(entity_ref.clone()), &entries);

        env.events()
            .publish((symbol_short!("sc_upd"), entry_id), entity_ref);

        Ok(())
    }

    /// Perform consistency check on a sidecar entry
    pub fn check_consistency(
        env: Env,
        entry_id: String,
        current_entity_hash: String,
    ) -> Result<ConsistencyCheck, SidecarError> {
        let entity_ref = Self::get_entity_for_entry(&env, &entry_id)?;
        let entries: Vec<SidecarEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::SidecarEntries(entity_ref))
            .ok_or(SidecarError::SidecarNotFound)?;

        for i in 0..entries.len() {
            if let Some(entry) = entries.get(i) {
                if entry.entry_id == entry_id {
                    let is_consistent = entry.entity_hash == current_entity_hash;
                    let check = ConsistencyCheck {
                        is_consistent,
                        expected_hash: entry.entity_hash.clone(),
                        actual_hash: current_entity_hash,
                        checked_at: env.ledger().timestamp(),
                    };
                    return Ok(check);
                }
            }
        }

        Err(SidecarError::SidecarNotFound)
    }

    /// Query all sidecar entries for an entity
    pub fn query_by_entity(env: Env, entity_ref: String) -> Vec<SidecarEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::SidecarEntries(entity_ref))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Query a specific sidecar entry by ID
    pub fn query_by_id(env: Env, entry_id: String) -> Result<SidecarEntry, SidecarError> {
        let entity_ref = Self::get_entity_for_entry(&env, &entry_id)?;
        let entries: Vec<SidecarEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::SidecarEntries(entity_ref))
            .ok_or(SidecarError::SidecarNotFound)?;

        for i in 0..entries.len() {
            if let Some(entry) = entries.get(i) {
                if entry.entry_id == entry_id {
                    return Ok(entry);
                }
            }
        }

        Err(SidecarError::SidecarNotFound)
    }

    /// Get all entities that have sidecar entries
    pub fn get_all_entities(env: Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::EntityIndex)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // =======================================================================
    // Internal helpers
    // =======================================================================

    fn get_entity_for_entry(env: &Env, entry_id: &String) -> Result<String, SidecarError> {
        env.storage()
            .persistent()
            .get(&DataKey::EntryToEntity(entry_id.clone()))
            .ok_or(SidecarError::InvalidReference)
    }

    fn add_to_entity_index(env: &Env, entity_ref: &String) {
        let mut index: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::EntityIndex)
            .unwrap_or_else(|| Vec::new(env));

        let mut exists = false;
        for i in 0..index.len() {
            if let Some(e) = index.get(i) {
                if e == *entity_ref {
                    exists = true;
                    break;
                }
            }
        }

        if !exists {
            index.push_back(entity_ref.clone());
            env.storage().instance().set(&DataKey::EntityIndex, &index);
        }
    }
}
