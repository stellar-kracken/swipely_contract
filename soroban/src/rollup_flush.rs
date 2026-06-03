//! Rollup Flush for Bridge Watch.
//!
//! Buffers rollup values (health scores and prices) and flushes them into
//! the contract state on demand. Supports both manual admin-triggered flushes
//! and provides a read-only view of the current buffer contents.

use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::keys;

/// Maximum number of assets that can be buffered simultaneously.
pub const MAX_BUFFER_SIZE: u32 = 50;

/// A single buffered rollup entry for one asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollupBuffer {
    pub asset_code: String,
    pub total_health_score: u64,
    pub total_price: i128,
    pub count: u32,
    pub last_updated: u64,
}

/// Result of a flush operation for one asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlushResult {
    pub asset_code: String,
    pub avg_health_score: u32,
    pub avg_price: i128,
    pub sample_count: u32,
    pub flushed_at: u64,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RollupFlushKey {
    /// Buffer data for a single asset.
    Buffer(String),
    /// List of all asset codes currently in the buffer.
    BufferedAssets,
    /// Last flush result for an asset.
    FlushResult(String),
}

// ── Internal Helpers ──────────────────────────────────────────────────────────

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    if *caller != admin {
        panic!("only admin can manage rollup buffer");
    }
}

fn load_buffered_assets(env: &Env) -> Vec<String> {
    env.storage()
        .instance()
        .get(&RollupFlushKey::BufferedAssets)
        .unwrap_or_else(|| Vec::new(env))
}

fn save_buffered_assets(env: &Env, assets: &Vec<String>) {
    env.storage()
        .instance()
        .set(&RollupFlushKey::BufferedAssets, assets);
}

// ── Core Functions ────────────────────────────────────────────────────────────

/// Add a data point to the rollup buffer for a given asset.
///
/// Each call accumulates the health_score and price values. The flush
/// operation later computes averages from the accumulated totals.
///
/// Admin only.
pub fn buffer_rollup_value(
    env: &Env,
    caller: &Address,
    asset_code: String,
    health_score: u32,
    price: i128,
) {
    require_admin(env, caller);

    let now = env.ledger().timestamp();
    let key = RollupFlushKey::Buffer(asset_code.clone());
    let existing: Option<RollupBuffer> = env.storage().persistent().get(&key);

    let buffer = match existing {
        Some(mut buf) => {
            buf.total_health_score += health_score as u64;
            buf.total_price += price;
            buf.count += 1;
            buf.last_updated = now;
            buf
        }
        None => {
            // Track asset in the buffered list
            let mut assets = load_buffered_assets(env);
            if assets.len() >= MAX_BUFFER_SIZE {
                panic!("rollup buffer is full");
            }
            let mut found = false;
            for a in assets.iter() {
                if a == asset_code {
                    found = true;
                    break;
                }
            }
            if !found {
                assets.push_back(asset_code.clone());
                save_buffered_assets(env, &assets);
            }

            RollupBuffer {
                asset_code: asset_code.clone(),
                total_health_score: health_score as u64,
                total_price: price,
                count: 1,
                last_updated: now,
            }
        }
    };

    env.storage().persistent().set(&key, &buffer);
}

/// Flush all buffered rollup values, computing averages and storing results.
///
/// Each buffered asset gets its average health score and price written to
/// a `FlushResult` record. The buffer is cleared after a successful flush.
///
/// Returns the list of flush results. Admin only.
///
/// If the buffer is empty, returns an empty list (no-op).
pub fn flush_rollup(env: &Env, caller: &Address) -> Vec<FlushResult> {
    require_admin(env, caller);

    let assets = load_buffered_assets(env);
    let now = env.ledger().timestamp();
    let mut results: Vec<FlushResult> = Vec::new(env);

    for asset_code in assets.iter() {
        let buf_key = RollupFlushKey::Buffer(asset_code.clone());
        let buffer: Option<RollupBuffer> = env.storage().persistent().get(&buf_key);

        if let Some(buf) = buffer {
            if buf.count == 0 {
                continue;
            }

            let avg_health = (buf.total_health_score / buf.count as u64) as u32;
            let avg_price = buf.total_price / buf.count as i128;

            let result = FlushResult {
                asset_code: asset_code.clone(),
                avg_health_score: avg_health,
                avg_price,
                sample_count: buf.count,
                flushed_at: now,
            };

            let result_key = RollupFlushKey::FlushResult(asset_code.clone());
            env.storage().persistent().set(&result_key, &result);
            results.push_back(result);

            // Clear the buffer entry
            env.storage().persistent().remove(&buf_key);
        }
    }

    // Clear the buffered assets list
    let empty: Vec<String> = Vec::new(env);
    save_buffered_assets(env, &empty);

    if results.len() > 0 {
        env.events()
            .publish((symbol_short!("rlp_fl"),), results.len());
    }

    results
}

/// Return the current contents of the rollup buffer without flushing.
///
/// Read-only.
pub fn get_rollup_buffer(env: &Env) -> Vec<RollupBuffer> {
    let assets = load_buffered_assets(env);
    let mut result: Vec<RollupBuffer> = Vec::new(env);

    for asset_code in assets.iter() {
        let key = RollupFlushKey::Buffer(asset_code.clone());
        if let Some(buf) = env.storage().persistent().get::<_, RollupBuffer>(&key) {
            result.push_back(buf);
        }
    }

    result
}

/// Return the list of asset codes currently in the buffer.
///
/// Read-only.
pub fn get_buffered_asset_codes(env: &Env) -> Vec<String> {
    load_buffered_assets(env)
}

/// Return the last flush result for a specific asset.
///
/// Read-only.
pub fn get_flush_result(env: &Env, asset_code: String) -> Option<FlushResult> {
    let key = RollupFlushKey::FlushResult(asset_code);
    env.storage().persistent().get(&key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&keys::ADMIN, &admin);
        env.ledger().set_timestamp(1_000_000);
        (env, admin)
    }

    #[test]
    fn test_buffer_single_value() {
        let (env, admin) = setup();
        let asset = String::from_str(&env, "USDC");

        buffer_rollup_value(&env, &admin, asset.clone(), 80, 1_000_000);

        let buf = get_rollup_buffer(&env);
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.get(0).unwrap().count, 1);
        assert_eq!(buf.get(0).unwrap().total_health_score, 80);
    }

    #[test]
    fn test_buffer_multiple_values_accumulate() {
        let (env, admin) = setup();
        let asset = String::from_str(&env, "USDC");

        buffer_rollup_value(&env, &admin, asset.clone(), 80, 1_000_000);
        buffer_rollup_value(&env, &admin, asset.clone(), 90, 1_100_000);

        let buf = get_rollup_buffer(&env);
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.get(0).unwrap().count, 2);
        assert_eq!(buf.get(0).unwrap().total_health_score, 170);
        assert_eq!(buf.get(0).unwrap().total_price, 2_100_000);
    }

    #[test]
    fn test_flush_computes_averages() {
        let (env, admin) = setup();
        let asset = String::from_str(&env, "USDC");

        buffer_rollup_value(&env, &admin, asset.clone(), 80, 1_000_000);
        buffer_rollup_value(&env, &admin, asset.clone(), 90, 1_100_000);
        buffer_rollup_value(&env, &admin, asset.clone(), 70, 900_000);

        let results = flush_rollup(&env, &admin);
        assert_eq!(results.len(), 1);

        let r = results.get(0).unwrap();
        assert_eq!(r.avg_health_score, 80); // (80+90+70)/3 = 80
        assert_eq!(r.avg_price, 1_000_000); // (1000000+1100000+900000)/3 = 1000000
        assert_eq!(r.sample_count, 3);
    }

    #[test]
    fn test_flush_clears_buffer() {
        let (env, admin) = setup();
        let asset = String::from_str(&env, "USDC");

        buffer_rollup_value(&env, &admin, asset.clone(), 80, 1_000_000);
        flush_rollup(&env, &admin);

        let buf = get_rollup_buffer(&env);
        assert_eq!(buf.len(), 0);

        let codes = get_buffered_asset_codes(&env);
        assert_eq!(codes.len(), 0);
    }

    #[test]
    fn test_flush_empty_buffer_is_noop() {
        let (env, admin) = setup();
        let results = flush_rollup(&env, &admin);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_flush_multiple_assets() {
        let (env, admin) = setup();
        let usdc = String::from_str(&env, "USDC");
        let xlm = String::from_str(&env, "XLM");

        buffer_rollup_value(&env, &admin, usdc.clone(), 80, 1_000_000);
        buffer_rollup_value(&env, &admin, xlm.clone(), 60, 500_000);

        let results = flush_rollup(&env, &admin);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_get_flush_result_after_flush() {
        let (env, admin) = setup();
        let asset = String::from_str(&env, "USDC");

        buffer_rollup_value(&env, &admin, asset.clone(), 80, 1_000_000);
        flush_rollup(&env, &admin);

        let result = get_flush_result(&env, asset);
        assert!(result.is_some());
        assert_eq!(result.unwrap().avg_health_score, 80);
    }

    #[test]
    fn test_deterministic_flush_results() {
        let (env, admin) = setup();
        let asset = String::from_str(&env, "USDC");

        buffer_rollup_value(&env, &admin, asset.clone(), 100, 2_000_000);
        buffer_rollup_value(&env, &admin, asset.clone(), 50, 1_000_000);

        let results = flush_rollup(&env, &admin);
        let r = results.get(0).unwrap();
        // (100+50)/2 = 75
        assert_eq!(r.avg_health_score, 75);
        // (2000000+1000000)/2 = 1500000
        assert_eq!(r.avg_price, 1_500_000);
    }

    #[test]
    #[should_panic(expected = "only admin")]
    fn test_non_admin_cannot_buffer() {
        let (env, _admin) = setup();
        let stranger = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");
        buffer_rollup_value(&env, &stranger, asset, 80, 1_000_000);
    }

    #[test]
    #[should_panic(expected = "only admin")]
    fn test_non_admin_cannot_flush() {
        let (env, _admin) = setup();
        let stranger = Address::generate(&env);
        flush_rollup(&env, &stranger);
    }
}
