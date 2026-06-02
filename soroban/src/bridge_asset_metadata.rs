//! Safe asset metadata updates for Bridge Watch monitored assets.
//!
//! Updates metadata in-place without recreating the asset registration entry.

use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::keys;

/// Maximum metadata field lengths (bytes).
pub const MAX_NAME_LEN: u32 = 128;
pub const MAX_SYMBOL_LEN: u32 = 32;
pub const MAX_DESCRIPTION_LEN: u32 = 512;
pub const MAX_URL_LEN: u32 = 256;
pub const MAX_REASON_LEN: u32 = 256;
pub const MAX_VERSION_HISTORY: u32 = 50;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataStorageKey {
    Meta(String),
    History(String),
}

/// Current metadata document for a monitored asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeAssetMetadata {
    pub asset_code: String,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub url: String,
    pub version: u32,
    pub updated_at: u64,
    pub updated_by: Address,
}

/// One historical metadata change record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataChangeRecord {
    pub version: u32,
    pub metadata: BridgeAssetMetadata,
    pub change_reason: String,
    pub changed_by: Address,
    pub timestamp: u64,
}

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    if *caller != admin {
        panic!("only admin can update asset metadata");
    }
}

fn validate_field(label: &str, value: &String, max_len: u32) {
    if value.len() == 0 {
        panic!("{} must not be empty", label);
    }
    if value.len() > max_len {
        panic!("{} exceeds maximum length", label);
    }
}

fn asset_is_registered(env: &Env, asset_code: &String) -> bool {
    env.storage()
        .persistent()
        .has(&crate::DataKey::AssetHealth(asset_code.clone()))
}

fn load_history(env: &Env, asset_code: &String) -> Vec<MetadataChangeRecord> {
    env.storage()
        .persistent()
        .get(&MetadataStorageKey::History(asset_code.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn save_history(env: &Env, asset_code: &String, history: Vec<MetadataChangeRecord>) {
    let mut trimmed = history;
    if trimmed.len() > MAX_VERSION_HISTORY {
        let mut next: Vec<MetadataChangeRecord> = Vec::new(env);
        for i in 1..trimmed.len() {
            next.push_back(trimmed.get(i).unwrap());
        }
        trimmed = next;
    }
    env.storage()
        .persistent()
        .set(&MetadataStorageKey::History(asset_code.clone()), &trimmed);
}

/// Read metadata for an asset. Returns `None` when no metadata has been set.
pub fn get_metadata(env: Env, asset_code: String) -> Option<BridgeAssetMetadata> {
    env.storage()
        .persistent()
        .get(&MetadataStorageKey::Meta(asset_code))
}

/// Read metadata change history for an asset.
pub fn get_metadata_history(env: Env, asset_code: String) -> Vec<MetadataChangeRecord> {
    load_history(&env, &asset_code)
}

/// Update asset metadata without recreating the asset registration entry.
pub fn update_metadata(
    env: Env,
    caller: Address,
    asset_code: String,
    name: String,
    symbol: String,
    description: String,
    url: String,
    change_reason: String,
) -> BridgeAssetMetadata {
    require_admin(&env, &caller);

    if !asset_is_registered(&env, &asset_code) {
        panic!("asset is not registered");
    }

    validate_field("name", &name, MAX_NAME_LEN);
    validate_field("symbol", &symbol, MAX_SYMBOL_LEN);
    validate_field("description", &description, MAX_DESCRIPTION_LEN);
    validate_field("url", &url, MAX_URL_LEN);
    validate_field("change_reason", &change_reason, MAX_REASON_LEN);

    let now = env.ledger().timestamp();
    let previous: Option<BridgeAssetMetadata> = get_metadata(env.clone(), asset_code.clone());
    let version = previous.as_ref().map(|m| m.version + 1).unwrap_or(1);

    let metadata = BridgeAssetMetadata {
        asset_code: asset_code.clone(),
        name,
        symbol,
        description,
        url,
        version,
        updated_at: now,
        updated_by: caller.clone(),
    };

    env.storage()
        .persistent()
        .set(&MetadataStorageKey::Meta(asset_code.clone()), &metadata);

    let mut history = load_history(&env, &asset_code);
    history.push_back(MetadataChangeRecord {
        version,
        metadata: metadata.clone(),
        change_reason,
        changed_by: caller.clone(),
        timestamp: now,
    });
    save_history(&env, &asset_code, history);

    env.events().publish(
        (symbol_short!("meta_up"), asset_code.clone()),
        (version, now),
    );

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DataKey;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    fn setup() -> (Env, Address, String) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&keys::ADMIN, &admin);
        env.ledger().set_timestamp(1_000_000);

        let asset_code = String::from_str(&env, "USDC");
        env.storage().persistent().set(
            &DataKey::AssetHealth(asset_code.clone()),
            &crate::AssetHealth {
                asset_code: asset_code.clone(),
                health_score: 0,
                liquidity_score: 0,
                price_stability_score: 0,
                bridge_uptime_score: 0,
                paused: false,
                active: true,
                timestamp: 1_000_000,
            },
        );

        (env, admin, asset_code)
    }

    #[test]
    fn test_update_metadata_creates_version_history() {
        let (env, admin, asset_code) = setup();

        let meta = update_metadata(
            env.clone(),
            admin.clone(),
            asset_code.clone(),
            String::from_str(&env, "USD Coin"),
            String::from_str(&env, "USDC"),
            String::from_str(&env, "Stablecoin"),
            String::from_str(&env, "https://circle.com/usdc"),
            String::from_str(&env, "Initial metadata"),
        );
        assert_eq!(meta.version, 1);

        let updated = update_metadata(
            env.clone(),
            admin,
            asset_code.clone(),
            String::from_str(&env, "USD Coin v2"),
            String::from_str(&env, "USDC"),
            String::from_str(&env, "Updated stablecoin"),
            String::from_str(&env, "https://circle.com"),
            String::from_str(&env, "Refresh copy"),
        );
        assert_eq!(updated.version, 2);

        let history = get_metadata_history(env, asset_code);
        assert_eq!(history.len(), 2);
    }

    #[test]
    #[should_panic(expected = "not registered")]
    fn test_update_metadata_unknown_asset_fails() {
        let (env, admin, _) = setup();
        update_metadata(
            env,
            admin,
            String::from_str(&env, "FAKE"),
            String::from_str(&env, "Fake"),
            String::from_str(&env, "FAKE"),
            String::from_str(&env, "desc"),
            String::from_str(&env, "https://example.com"),
            String::from_str(&env, "reason"),
        );
    }
}
