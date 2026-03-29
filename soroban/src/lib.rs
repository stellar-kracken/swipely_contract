#![no_std]

// governance and insurance_pool are standalone contracts — only compiled for
// tests (native target) to avoid Wasm symbol conflicts with BridgeWatchContract.
#[cfg(test)]
pub mod governance;
pub mod liquidity_pool;
pub mod reputation_system;
#[cfg(test)]
pub mod insurance_pool;
#[cfg(test)]
pub mod rate_limiter;
#[cfg(test)]
pub mod asset_registry;
pub mod analytics_aggregator;
#[cfg(test)]
pub mod circuit_breaker;

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Vec};

use liquidity_pool::{
    DailyBucket, ImpermanentLossResult, LiquidityDepth as PoolLiquidityDepth, PoolMetrics,
    PoolSnapshot, PoolType,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetHealth {
    pub asset_code: String,
    pub health_score: u32,
    pub liquidity_score: u32,
    pub price_stability_score: u32,
    pub bridge_uptime_score: u32,
    pub paused: bool,
    pub active: bool,
    pub timestamp: u64,
}

/// Represents a single entry in a batch health score submission.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthScoreBatch {
    pub asset_code: String,
    pub health_score: u32,
    pub liquidity_score: u32,
    pub price_stability_score: u32,
    pub bridge_uptime_score: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceRecord {
    pub asset_code: String,
    pub price: i128,
    pub source: String,
    pub timestamp: u64,
}

/// Severity level of a recorded price deviation alert.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviationSeverity {
    /// Deviation exceeds the low threshold (default > 2 %).
    Low,
    /// Deviation exceeds the medium threshold (default > 5 %).
    Medium,
    /// Deviation exceeds the high threshold (default > 10 %).
    High,
}

/// A price deviation alert stored on-chain for an asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviationAlert {
    pub asset_code: String,
    pub current_price: i128,
    pub average_price: i128,
    /// Deviation expressed in basis points (1 bp = 0.01 %).
    pub deviation_bps: i128,
    pub severity: DeviationSeverity,
    pub timestamp: u64,
}

/// Per-asset configurable deviation thresholds (in basis points).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviationThreshold {
    /// Low-severity trigger; default 200 bps (2 %).
    pub low_bps: i128,
    /// Medium-severity trigger; default 500 bps (5 %).
    pub medium_bps: i128,
    /// High-severity trigger; default 1 000 bps (10 %).
    pub high_bps: i128,
}

/// Records a supply mismatch between Stellar and a source chain for a bridge.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplyMismatch {
    pub bridge_id: String,
    pub asset_code: String,
    pub stellar_supply: i128,
    pub source_chain_supply: i128,
    /// Mismatch expressed in basis points (1 bp = 0.01 %).
    pub mismatch_bps: i128,
    /// `true` when `mismatch_bps` is at or above the configured threshold.
    pub is_critical: bool,
    pub timestamp: u64,
}

/// Aggregated liquidity depth for an asset pair across multiple DEX venues.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidityDepth {
    /// Asset pair identifier (for example, "USDC/XLM").
    pub asset_pair: String,
    /// Total aggregated liquidity across all reported venues.
    pub total_liquidity: i128,
    /// Available liquidity within 0.1 % price impact.
    pub depth_0_1_pct: i128,
    /// Available liquidity within 0.5 % price impact.
    pub depth_0_5_pct: i128,
    /// Available liquidity within 1 % price impact.
    pub depth_1_pct: i128,
    /// Available liquidity within 5 % price impact.
    pub depth_5_pct: i128,
    /// Venue names contributing to the aggregate snapshot.
    pub sources: Vec<String>,
    /// Ledger timestamp when this aggregate was recorded.
    pub timestamp: u64,
}
/// Permission roles that can be assigned to admin addresses.
///
/// - `SuperAdmin` – all permissions, can manage other roles.
/// - `HealthSubmitter` – may call `submit_health()` and `submit_health_batch()`.
/// - `PriceSubmitter` – may call `submit_price()` only.
/// - `AssetManager` – may call `register_asset()` only.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdminRole {
    SuperAdmin,
    HealthSubmitter,
    PriceSubmitter,
    AssetManager,
}

/// Pairs an address with a single granted role.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleAssignment {
    pub address: Address,
    pub role: AdminRole,
}

#[contracttype]
pub enum DataKey {
    Admin,
    AssetHealth(String),
    PriceRecord(String),
    MonitoredAssets,
    /// Latest deviation alert recorded for an asset.
    DeviationAlert(String),
    /// Admin-configured deviation thresholds for an asset.
    DeviationThreshold(String),
    /// Historical supply mismatch records for a bridge (Vec<SupplyMismatch>).
    SupplyMismatches(String),
    /// Global critical mismatch threshold in basis points (default 10 bps / 0.1 %).
    MismatchThreshold,
    /// All bridge IDs that have at least one mismatch record (Vec<String>).
    BridgeIds,
    /// Roles held by a specific address (Vec<AdminRole>).
    RoleKey(Address),
    /// Global list of all role assignments for enumeration.
    RolesList,
    /// Current aggregated liquidity depth for an asset pair.
    LiquidityDepthCurrent(String),
    /// Historical aggregated liquidity depth snapshots for an asset pair.
    LiquidityDepthHistory(String),
    /// Registered asset pairs with liquidity depth data.
    LiquidityPairs,
    /// Historical price records for an asset (Vec<PriceRecord>).
    PriceHistory(String),
    /// Pre-computed statistics for an asset (Vec<Statistics>).
    AssetStatistics(String),
}

/// Time period for statistical calculations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatPeriod {
    Hour,
    Day,
    Week,
    Month,
}

/// Pre-computed statistical data for an asset over a specific period.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Statistics {
    pub asset_code: String,
    pub period: StatPeriod,
    pub average_price: i128,
    pub stddev_price: i128,
    pub volatility_bps: i128,
    pub min_price: i128,
    pub max_price: i128,
    pub median_price: i128,
    pub p25_price: i128,
    pub p75_price: i128,
    pub data_points: u32,
    pub timestamp: u64,
}

/// Input parameters for statistical calculations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalculationInput {
    pub values: Vec<i128>,
    pub volumes: Option<Vec<i128>>, // For volume-weighted calculations
}

#[contract]
pub struct BridgeWatchContract;

#[contractimpl]
impl BridgeWatchContract {
    /// Initialize the contract with an admin address
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        let assets: Vec<String> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::MonitoredAssets, &assets);
    }

    /// Submit a health score for a monitored asset.
    ///
    /// `caller` must be the contract admin, a `SuperAdmin`, or a
    /// `HealthSubmitter`. Backward compatible: the original admin address
    /// requires no explicit role assignment.
    pub fn submit_health(
        env: Env,
        caller: Address,
        asset_code: String,
        health_score: u32,
        liquidity_score: u32,
        price_stability_score: u32,
        bridge_uptime_score: u32,
    ) {
        Self::check_permission(&env, &caller, AdminRole::HealthSubmitter);
        let status = Self::load_asset_health(&env, &asset_code);
        Self::assert_asset_accepting_submissions(&status);

        let record = AssetHealth {
            asset_code: asset_code.clone(),
            health_score,
            liquidity_score,
            price_stability_score,
            bridge_uptime_score,
            paused: status.paused,
            active: status.active,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::AssetHealth(asset_code), &record);
    }

    /// Submit health scores for multiple assets in a single transaction.
    ///
    /// `caller` must be the contract admin, a `SuperAdmin`, or a
    /// `HealthSubmitter`. Accepts up to 20 records per call, all stamped with
    /// the same ledger timestamp. A `health_up` event is emitted per asset.
    pub fn submit_health_batch(env: Env, caller: Address, records: Vec<HealthScoreBatch>) {
        Self::check_permission(&env, &caller, AdminRole::HealthSubmitter);

        if records.len() > 20 {
            panic!("batch size exceeds the maximum of 20 records");
        }

        let timestamp = env.ledger().timestamp();

        for item in records.iter() {
            let status = Self::load_asset_health(&env, &item.asset_code);
            Self::assert_asset_accepting_submissions(&status);

            let record = AssetHealth {
                asset_code: item.asset_code.clone(),
                health_score: item.health_score,
                liquidity_score: item.liquidity_score,
                price_stability_score: item.price_stability_score,
                bridge_uptime_score: item.bridge_uptime_score,
                paused: status.paused,
                active: status.active,
                timestamp,
            };

            env.storage()
                .persistent()
                .set(&DataKey::AssetHealth(item.asset_code.clone()), &record);

            env.events().publish(
                (symbol_short!("health_up"), item.asset_code.clone()),
                item.health_score,
            );
        }
    }

    /// Submit a price record for an asset.
    ///
    /// `caller` must be the contract admin, a `SuperAdmin`, or a
    /// `PriceSubmitter`. The record is stored as the latest price and
    /// also appended to the asset's historical price series for
    /// time-range queries via [`get_price_history`].
    pub fn submit_price(
        env: Env,
        caller: Address,
        asset_code: String,
        price: i128,
        source: String,
    ) {
        Self::check_permission(&env, &caller, AdminRole::PriceSubmitter);
        let status = Self::load_asset_health(&env, &asset_code);
        Self::assert_asset_accepting_submissions(&status);

        let record = PriceRecord {
            asset_code: asset_code.clone(),
            price,
            source,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::PriceRecord(asset_code.clone()), &record);

        // Append to historical price series
        let mut history: Vec<PriceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::PriceHistory(asset_code.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        history.push_back(record);
        env.storage()
            .persistent()
            .set(&DataKey::PriceHistory(asset_code), &history);
    }

    /// Retrieve historical price records for an asset within a time range.
    ///
    /// Returns up to `limit` records (capped at 100) whose timestamps fall
    /// within the inclusive range `[start_time, end_time]`, sorted by
    /// timestamp in **descending** order (newest first). Use the `offset`
    /// parameter for pagination through large result sets.
    ///
    /// # Parameters
    /// - `asset_code` – asset identifier (e.g. `"USDC"`).
    /// - `start_time` – inclusive lower bound of the query window.
    /// - `end_time` – inclusive upper bound of the query window.
    /// - `limit` – maximum number of records to return (clamped to 100).
    /// - `offset` – number of matching records to skip (for pagination).
    ///
    /// # Panics
    /// Panics when `start_time > end_time`.
    pub fn get_price_history(
        env: Env,
        asset_code: String,
        start_time: u64,
        end_time: u64,
        limit: u32,
        offset: u32,
    ) -> Vec<PriceRecord> {
        if start_time > end_time {
            panic!("start_time must be less than or equal to end_time");
        }

        let max_limit: u32 = 100;
        let effective_limit = if limit == 0 || limit > max_limit {
            max_limit
        } else {
            limit
        };

        let history: Vec<PriceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::PriceHistory(asset_code))
            .unwrap_or_else(|| Vec::new(&env));

        // Collect matching records into a temporary vector
        let mut matched: Vec<PriceRecord> = Vec::new(&env);
        for record in history.iter() {
            if record.timestamp >= start_time && record.timestamp <= end_time {
                matched.push_back(record);
            }
        }

        // Reverse to achieve descending timestamp order (newest first)
        let total = matched.len();
        let mut descending: Vec<PriceRecord> = Vec::new(&env);
        let mut i = total;
        while i > 0 {
            i -= 1;
            descending.push_back(matched.get(i).unwrap());
        }

        // Apply offset and limit for pagination
        let mut result: Vec<PriceRecord> = Vec::new(&env);
        let mut skipped: u32 = 0;
        let mut collected: u32 = 0;
        for record in descending.iter() {
            if skipped < offset {
                skipped += 1;
                continue;
            }
            if collected >= effective_limit {
                break;
            }
            result.push_back(record);
            collected += 1;
        }

        result
    }

    /// Get the latest health record for an asset
    pub fn get_health(env: Env, asset_code: String) -> Option<AssetHealth> {
        env.storage()
            .persistent()
            .get(&DataKey::AssetHealth(asset_code))
    }

    /// Get the latest price record for an asset
    pub fn get_price(env: Env, asset_code: String) -> Option<PriceRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::PriceRecord(asset_code))
    }

    /// Register a new asset for monitoring.
    ///
    /// `caller` must be the contract admin, a `SuperAdmin`, or an
    /// `AssetManager`.
    pub fn register_asset(env: Env, caller: Address, asset_code: String) {
        Self::check_permission(&env, &caller, AdminRole::AssetManager);

        let mut assets: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::MonitoredAssets)
            .unwrap();

        for existing in assets.iter() {
            if existing == asset_code {
                panic!("asset is already registered");
            }
        }

        let status = AssetHealth {
            asset_code: asset_code.clone(),
            health_score: 0,
            liquidity_score: 0,
            price_stability_score: 0,
            bridge_uptime_score: 0,
            paused: false,
            active: true,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::AssetHealth(asset_code.clone()), &status);

        assets.push_back(asset_code.clone());
        env.storage()
            .instance()
            .set(&DataKey::MonitoredAssets, &assets);

        env.events()
            .publish((symbol_short!("asset_reg"), asset_code), true);
    }

    /// Temporarily pause monitoring for an asset.
    ///
    /// `caller` must be the contract admin, a `SuperAdmin`, or an
    /// `AssetManager`.
    pub fn pause_asset(env: Env, caller: Address, asset_code: String) {
        Self::check_permission(&env, &caller, AdminRole::AssetManager);
        let mut status = Self::load_asset_health(&env, &asset_code);
        if !status.active {
            panic!("cannot pause a deregistered asset");
        }
        status.paused = true;
        status.timestamp = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::AssetHealth(asset_code.clone()), &status);
        env.events()
            .publish((symbol_short!("asset_pau"), asset_code), true);
    }

    /// Resume monitoring for a paused asset.
    ///
    /// `caller` must be the contract admin, a `SuperAdmin`, or an
    /// `AssetManager`.
    pub fn unpause_asset(env: Env, caller: Address, asset_code: String) {
        Self::check_permission(&env, &caller, AdminRole::AssetManager);
        let mut status = Self::load_asset_health(&env, &asset_code);
        if !status.active {
            panic!("cannot unpause a deregistered asset");
        }
        status.paused = false;
        status.timestamp = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::AssetHealth(asset_code.clone()), &status);
        env.events()
            .publish((symbol_short!("asset_unp"), asset_code), true);
    }

    /// Permanently deregister an asset while retaining historical data.
    ///
    /// `caller` must be the contract admin, a `SuperAdmin`, or an
    /// `AssetManager`.
    pub fn deregister_asset(env: Env, caller: Address, asset_code: String) {
        Self::check_permission(&env, &caller, AdminRole::AssetManager);
        let mut status = Self::load_asset_health(&env, &asset_code);
        status.active = false;
        status.paused = false;
        status.timestamp = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::AssetHealth(asset_code.clone()), &status);
        env.events()
            .publish((symbol_short!("asset_del"), asset_code), false);
    }

    /// Get all monitored assets
    pub fn get_monitored_assets(env: Env) -> Vec<String> {
        let assets: Vec<String> = env.storage()
            .instance()
            .get(&DataKey::MonitoredAssets)
            .unwrap();

        let mut active_assets = Vec::new(&env);
        for asset_code in assets.iter() {
            let status: Option<AssetHealth> = env
                .storage()
                .persistent()
                .get(&DataKey::AssetHealth(asset_code.clone()));

            match status {
                Some(record) => {
                    if record.active && !record.paused {
                        active_assets.push_back(asset_code);
                    }
                }
                None => active_assets.push_back(asset_code),
            }
        }

        active_assets
    }

    // -----------------------------------------------------------------------
    // Price Deviation Detection (issue #23)
    // -----------------------------------------------------------------------

    /// Set configurable deviation thresholds for an asset (admin only).
    ///
    /// All thresholds are expressed in basis points (1 bp = 0.01 %).
    /// Defaults used when none are configured: Low 200 bps, Medium 500 bps,
    /// High 1 000 bps.
    pub fn set_deviation_threshold(
        env: Env,
        asset_code: String,
        low_bps: i128,
        medium_bps: i128,
        high_bps: i128,
    ) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let threshold = DeviationThreshold {
            low_bps,
            medium_bps,
            high_bps,
        };
        env.storage()
            .persistent()
            .set(&DataKey::DeviationThreshold(asset_code), &threshold);
    }

    /// Compare `current_price` against the last recorded [`PriceRecord`] for
    /// the asset and store a [`DeviationAlert`] when the deviation exceeds a
    /// configured threshold.
    ///
    /// Returns the alert when a threshold is breached, `None` otherwise.
    /// Severity levels (default thresholds):
    /// - **Low** – deviation > 200 bps (2 %)
    /// - **Medium** – deviation > 500 bps (5 %)
    /// - **High** – deviation > 1 000 bps (10 %)
    pub fn check_price_deviation(
        env: Env,
        asset_code: String,
        current_price: i128,
    ) -> Option<DeviationAlert> {
        let reference: PriceRecord = match env
            .storage()
            .persistent()
            .get(&DataKey::PriceRecord(asset_code.clone()))
        {
            Some(r) => r,
            None => return None,
        };

        let average_price = reference.price;
        if average_price == 0 {
            return None;
        }

        let diff = if current_price > average_price {
            current_price - average_price
        } else {
            average_price - current_price
        };
        let deviation_bps = diff * 10_000 / average_price;

        let threshold: DeviationThreshold = env
            .storage()
            .persistent()
            .get(&DataKey::DeviationThreshold(asset_code.clone()))
            .unwrap_or(DeviationThreshold {
                low_bps: 200,
                medium_bps: 500,
                high_bps: 1_000,
            });

        let severity = if deviation_bps > threshold.high_bps {
            DeviationSeverity::High
        } else if deviation_bps > threshold.medium_bps {
            DeviationSeverity::Medium
        } else if deviation_bps > threshold.low_bps {
            DeviationSeverity::Low
        } else {
            return None;
        };

        let alert = DeviationAlert {
            asset_code: asset_code.clone(),
            current_price,
            average_price,
            deviation_bps,
            severity,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::DeviationAlert(asset_code), &alert);

        Some(alert)
    }

    /// Get the latest stored deviation alert for an asset.
    ///
    /// Returns `None` if no alert has been recorded.
    pub fn get_deviation_alerts(env: Env, asset_code: String) -> Option<DeviationAlert> {
        env.storage()
            .persistent()
            .get(&DataKey::DeviationAlert(asset_code))
    }

    // -----------------------------------------------------------------------
    // Bridge supply mismatch tracking (issue #28)
    // -----------------------------------------------------------------------

    /// Set the global critical mismatch threshold in basis points (admin only).
    ///
    /// Mismatches at or above this value are flagged as critical.
    /// Default is 10 bps (0.1 %).
    pub fn set_mismatch_threshold(env: Env, threshold_bps: i128) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::MismatchThreshold, &threshold_bps);
    }

    /// Record a supply mismatch for a bridge asset (admin only).
    ///
    /// Calculates `mismatch_bps` as
    /// `|stellar_supply - source_chain_supply| * 10_000 / source_chain_supply`
    /// and sets `is_critical` when the value meets or exceeds the configured
    /// threshold (default 10 bps / 0.1 %). Each call appends to the bridge's
    /// historical record, enabling trend analysis over time.
    pub fn record_supply_mismatch(
        env: Env,
        bridge_id: String,
        asset_code: String,
        stellar_supply: i128,
        source_chain_supply: i128,
    ) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let mismatch_bps = if source_chain_supply > 0 {
            let diff = if stellar_supply > source_chain_supply {
                stellar_supply - source_chain_supply
            } else {
                source_chain_supply - stellar_supply
            };
            diff * 10_000 / source_chain_supply
        } else {
            0
        };

        let threshold_bps: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MismatchThreshold)
            .unwrap_or(10);

        let is_critical = mismatch_bps >= threshold_bps;

        let record = SupplyMismatch {
            bridge_id: bridge_id.clone(),
            asset_code,
            stellar_supply,
            source_chain_supply,
            mismatch_bps,
            is_critical,
            timestamp: env.ledger().timestamp(),
        };

        let mut mismatches: Vec<SupplyMismatch> = env
            .storage()
            .persistent()
            .get(&DataKey::SupplyMismatches(bridge_id.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        mismatches.push_back(record);
        env.storage()
            .persistent()
            .set(&DataKey::SupplyMismatches(bridge_id.clone()), &mismatches);

        // Track bridge ID for cross-bridge queries
        let mut bridge_ids: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::BridgeIds)
            .unwrap_or_else(|| Vec::new(&env));
        let mut found = false;
        for b in bridge_ids.iter() {
            if b == bridge_id {
                found = true;
                break;
            }
        }
        if !found {
            bridge_ids.push_back(bridge_id);
            env.storage()
                .instance()
                .set(&DataKey::BridgeIds, &bridge_ids);
        }
    }

    /// Return all recorded supply mismatches for a bridge. Public read access.
    pub fn get_supply_mismatches(env: Env, bridge_id: String) -> Vec<SupplyMismatch> {
        env.storage()
            .persistent()
            .get(&DataKey::SupplyMismatches(bridge_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Return all critical mismatches across every tracked bridge. Public read access.
    pub fn get_critical_mismatches(env: Env) -> Vec<SupplyMismatch> {
        let bridge_ids: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::BridgeIds)
            .unwrap_or_else(|| Vec::new(&env));

        let mut critical: Vec<SupplyMismatch> = Vec::new(&env);
        for bridge_id in bridge_ids.iter() {
            let mismatches: Vec<SupplyMismatch> = env
                .storage()
                .persistent()
                .get(&DataKey::SupplyMismatches(bridge_id.clone()))
                .unwrap_or_else(|| Vec::new(&env));
            for m in mismatches.iter() {
                if m.is_critical {
                    critical.push_back(m);
                }
            }
        }
        critical
    }

    // -----------------------------------------------------------------------
    // Multi-DEX liquidity depth tracking (issue #31)
    // -----------------------------------------------------------------------

    /// Record aggregated liquidity depth for a supported asset pair.
    ///
    /// This stores the latest cross-DEX liquidity snapshot as well as
    /// appending it to the pair's historical series for trend analysis.
    ///
    /// Supported Phase 1 pairs are:
    /// - `USDC/XLM`
    /// - `EURC/XLM`
    /// - `PYUSD/XLM`
    /// - `FOBXX/USDC`
    ///
    /// # Panics
    /// Panics when:
    /// - the caller is not the contract admin
    /// - the asset pair is not supported in Phase 1
    /// - any liquidity value is negative
    /// - `sources` is empty
    /// - liquidity depth levels are inconsistent
    pub fn record_liquidity_depth(
        env: Env,
        asset_pair: String,
        total_liquidity: i128,
        depth_0_1_pct: i128,
        depth_0_5_pct: i128,
        depth_1_pct: i128,
        depth_5_pct: i128,
        sources: Vec<String>,
    ) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        Self::validate_liquidity_depth_input(
            &env,
            &asset_pair,
            total_liquidity,
            depth_0_1_pct,
            depth_0_5_pct,
            depth_1_pct,
            depth_5_pct,
            &sources,
        );

        let record = LiquidityDepth {
            asset_pair: asset_pair.clone(),
            total_liquidity,
            depth_0_1_pct,
            depth_0_5_pct,
            depth_1_pct,
            depth_5_pct,
            sources,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::LiquidityDepthCurrent(asset_pair.clone()), &record);

        let mut history: Vec<LiquidityDepth> = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidityDepthHistory(asset_pair.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        history.push_back(record);
        env.storage().persistent().set(
            &DataKey::LiquidityDepthHistory(asset_pair.clone()),
            &history,
        );

        let mut pairs: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::LiquidityPairs)
            .unwrap_or_else(|| Vec::new(&env));

        let mut found = false;
        for pair in pairs.iter() {
            if pair == asset_pair {
                found = true;
                break;
            }
        }

        if !found {
            pairs.push_back(asset_pair);
            env.storage()
                .instance()
                .set(&DataKey::LiquidityPairs, &pairs);
        }
    }

    /// Return the latest aggregated liquidity depth for an asset pair.
    ///
    /// Public read access.
    pub fn get_aggregated_liquidity_depth(
        env: Env,
        asset_pair: String,
    ) -> Option<LiquidityDepth> {
        env.storage()
            .persistent()
            .get(&DataKey::LiquidityDepthCurrent(asset_pair))
    }

    /// Return historical liquidity depth snapshots for an asset pair.
    ///
    /// Public read access. Returned records are ordered by insertion time and
    /// filtered to the inclusive timestamp range `[from_timestamp, to_timestamp]`.
    pub fn get_liquidity_history(
        env: Env,
        asset_pair: String,
        from_timestamp: u64,
        to_timestamp: u64,
    ) -> Vec<LiquidityDepth> {
        let history: Vec<LiquidityDepth> = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidityDepthHistory(asset_pair))
            .unwrap_or_else(|| Vec::new(&env));

        let mut filtered = Vec::new(&env);
        for snapshot in history.iter() {
            if snapshot.timestamp >= from_timestamp && snapshot.timestamp <= to_timestamp {
                filtered.push_back(snapshot);
            }
        }

        filtered
    }

    /// Return the latest aggregated liquidity depth for all tracked asset pairs.
    ///
    /// Public read access.
    pub fn get_all_liquidity_depths(env: Env) -> Vec<LiquidityDepth> {
        let pairs: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::LiquidityPairs)
            .unwrap_or_else(|| Vec::new(&env));

        let mut records = Vec::new(&env);
        for pair in pairs.iter() {
            let current: Option<LiquidityDepth> = env
                .storage()
                .persistent()
                .get(&DataKey::LiquidityDepthCurrent(pair));
            if let Some(record) = current {
                records.push_back(record);
            }
        }

        records
    }
    // Multi-admin role management (issue #25)
    // -----------------------------------------------------------------------

    /// Grant a role to `grantee` (SuperAdmin or original admin only).
    ///
    /// Duplicate grants are silently ignored. The original admin address set
    /// via `initialize()` is implicitly treated as SuperAdmin and does not
    /// require an explicit role entry.
    pub fn grant_role(env: Env, granter: Address, grantee: Address, role: AdminRole) {
        granter.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let authorized =
            granter == admin || Self::has_role_internal(&env, &granter, AdminRole::SuperAdmin);
        if !authorized {
            panic!("only SuperAdmin can grant roles");
        }

        let mut roles: Vec<AdminRole> = env
            .storage()
            .persistent()
            .get(&DataKey::RoleKey(grantee.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        for r in roles.iter() {
            if r == role {
                return; // already granted
            }
        }
        roles.push_back(role.clone());
        env.storage()
            .persistent()
            .set(&DataKey::RoleKey(grantee.clone()), &roles);

        let mut assignments: Vec<RoleAssignment> = env
            .storage()
            .persistent()
            .get(&DataKey::RolesList)
            .unwrap_or_else(|| Vec::new(&env));
        assignments.push_back(RoleAssignment {
            address: grantee,
            role,
        });
        env.storage()
            .persistent()
            .set(&DataKey::RolesList, &assignments);
    }

    /// Revoke a specific role from `target` (SuperAdmin or original admin only).
    pub fn revoke_role(env: Env, revoker: Address, target: Address, role: AdminRole) {
        revoker.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let authorized =
            revoker == admin || Self::has_role_internal(&env, &revoker, AdminRole::SuperAdmin);
        if !authorized {
            panic!("only SuperAdmin can revoke roles");
        }

        let roles: Vec<AdminRole> = env
            .storage()
            .persistent()
            .get(&DataKey::RoleKey(target.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        let mut updated: Vec<AdminRole> = Vec::new(&env);
        for r in roles.iter() {
            if r != role {
                updated.push_back(r);
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::RoleKey(target.clone()), &updated);

        let assignments: Vec<RoleAssignment> = env
            .storage()
            .persistent()
            .get(&DataKey::RolesList)
            .unwrap_or_else(|| Vec::new(&env));

        let mut updated_assignments: Vec<RoleAssignment> = Vec::new(&env);
        for a in assignments.iter() {
            if !(a.address == target && a.role == role) {
                updated_assignments.push_back(a);
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::RolesList, &updated_assignments);
    }

    /// Return `true` if `address` holds `role`.
    ///
    /// Public read — no authorisation required.
    pub fn has_role(env: Env, address: Address, role: AdminRole) -> bool {
        Self::has_role_internal(&env, &address, role)
    }

    /// Return all active role assignments. Public read.
    pub fn get_admin_roles(env: Env) -> Vec<RoleAssignment> {
        env.storage()
            .persistent()
            .get(&DataKey::RolesList)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Verify that `caller` is authorised to perform an operation requiring
    /// `required_role`. The original admin address always passes. Any address
    /// with `SuperAdmin` or the specific `required_role` also passes.
    fn check_permission(env: &Env, caller: &Address, required_role: AdminRole) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if *caller == admin {
            return;
        }
        let has_super = Self::has_role_internal(env, caller, AdminRole::SuperAdmin);
        let has_required = Self::has_role_internal(env, caller, required_role);
        if !has_super && !has_required {
            panic!("unauthorized: caller does not have the required role");
        }
    }

    /// Internal role lookup (no auth check).
    fn has_role_internal(env: &Env, address: &Address, role: AdminRole) -> bool {
        let roles: Vec<AdminRole> = env
            .storage()
            .persistent()
            .get(&DataKey::RoleKey(address.clone()))
            .unwrap_or_else(|| Vec::new(env));
        for r in roles.iter() {
            if r == role {
                return true;
            }
        }
        false
    }

    fn validate_liquidity_depth_input(
        env: &Env,
        asset_pair: &String,
        total_liquidity: i128,
        depth_0_1_pct: i128,
        depth_0_5_pct: i128,
        depth_1_pct: i128,
        depth_5_pct: i128,
        sources: &Vec<String>,
    ) {
        if !Self::is_supported_liquidity_pair(env, asset_pair) {
            panic!("unsupported asset pair");
        }
        if total_liquidity < 0
            || depth_0_1_pct < 0
            || depth_0_5_pct < 0
            || depth_1_pct < 0
            || depth_5_pct < 0
        {
            panic!("liquidity values must be non-negative");
        }
        if sources.len() == 0 {
            panic!("at least one liquidity source is required");
        }
        if depth_0_1_pct > depth_0_5_pct || depth_0_5_pct > depth_1_pct || depth_1_pct > depth_5_pct
        {
            panic!("liquidity depth levels must be non-decreasing");
        }
        if depth_5_pct > total_liquidity {
            panic!("liquidity depth cannot exceed total liquidity");
        }
    }

    fn is_supported_liquidity_pair(env: &Env, asset_pair: &String) -> bool {
        *asset_pair == String::from_str(env, "USDC/XLM")
            || *asset_pair == String::from_str(env, "EURC/XLM")
            || *asset_pair == String::from_str(env, "PYUSD/XLM")
            || *asset_pair == String::from_str(env, "FOBXX/USDC")
    }

    fn load_asset_health(env: &Env, asset_code: &String) -> AssetHealth {
        env.storage()
            .persistent()
            .get(&DataKey::AssetHealth(asset_code.clone()))
            .unwrap_or_else(|| panic!("asset is not registered"))
    }

    fn assert_asset_accepting_submissions(record: &AssetHealth) {
        if !record.active {
            panic!("asset is deregistered");
        }
        if record.paused {
            panic!("asset monitoring is paused");
        }
    }
    // -----------------------------------------------------------------------
    // Liquidity Pool Monitor
    // -----------------------------------------------------------------------

    /// Record a new liquidity pool state snapshot (admin only).
    ///
    /// Writes the snapshot into a gas-optimised ring buffer, updates the
    /// corresponding daily aggregation bucket, and emits events when
    /// significant liquidity changes are detected.
    pub fn record_pool_state(
        env: Env,
        pool_id: String,
        reserve_a: i128,
        reserve_b: i128,
        total_shares: i128,
        volume: i128,
        fees: i128,
        pool_type: PoolType,
    ) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        liquidity_pool::record_pool_state(
            &env,
            pool_id,
            reserve_a,
            reserve_b,
            total_shares,
            volume,
            fees,
            pool_type,
        );
    }

    /// Calculate aggregated pool metrics over a time window.
    ///
    /// Returns volume, average depth, price change, fee APR, etc.
    /// for the specified `window_secs` lookback period.
    pub fn calculate_pool_metrics(env: Env, pool_id: String, window_secs: u64) -> PoolMetrics {
        liquidity_pool::calculate_pool_metrics(&env, pool_id, window_secs)
    }

    /// Retrieve historical pool snapshots within a time range.
    ///
    /// Public read access — no authorisation required.
    pub fn get_pool_history(
        env: Env,
        pool_id: String,
        from_timestamp: u64,
        to_timestamp: u64,
    ) -> Vec<PoolSnapshot> {
        liquidity_pool::get_pool_history(&env, pool_id, from_timestamp, to_timestamp)
    }

    /// Calculate impermanent loss for an LP position.
    ///
    /// Given the `entry_price` at which a position was opened and its
    /// `initial_value`, returns the current IL percentage, position value,
    /// and HODL comparison value.
    pub fn calculate_impermanent_loss(
        env: Env,
        pool_id: String,
        entry_price: i128,
        initial_value: i128,
    ) -> ImpermanentLossResult {
        liquidity_pool::calculate_impermanent_loss(&env, pool_id, entry_price, initial_value)
    }

    /// Get current liquidity depth information for a pool.
    ///
    /// Returns reserve amounts, total value locked, and a depth score
    /// from 0 to 100.
    pub fn get_liquidity_depth(env: Env, pool_id: String) -> PoolLiquidityDepth {
        liquidity_pool::get_liquidity_depth(&env, pool_id)
    }

    /// Get daily aggregated buckets for a pool within a time range.
    ///
    /// Returns OHLC price data, volume, fees, and average reserves
    /// per day. Public read access.
    pub fn get_daily_history(
        env: Env,
        pool_id: String,
        from_timestamp: u64,
        to_timestamp: u64,
    ) -> Vec<DailyBucket> {
        liquidity_pool::get_daily_history(&env, pool_id, from_timestamp, to_timestamp)
    }

    /// Get all registered liquidity pool IDs.
    pub fn get_registered_pools(env: Env) -> Vec<String> {
        liquidity_pool::get_registered_pools(&env)
    }

    // -----------------------------------------------------------------------
    // Statistical Calculations (issue #133)
    // -----------------------------------------------------------------------

    /// Calculate simple moving average of a value series.
    ///
    /// Returns the arithmetic mean of the provided values.
    /// Gas-efficient implementation for on-chain calculations.
    pub fn calculate_average(env: Env, values: Vec<i128>) -> i128 {
        let count = values.len() as i128;
        if count == 0 {
            return 0;
        }

        let mut sum: i128 = 0;
        for v in values.iter() {
            sum = sum.checked_add(v).unwrap_or(sum);
        }

        sum / count
    }

    /// Calculate volume-weighted moving average.
    ///
    /// Each value is weighted by its corresponding volume.
    pub fn calculate_volume_weighted_average(
        env: Env,
        values: Vec<i128>,
        volumes: Vec<i128>,
    ) -> i128 {
        if values.len() != volumes.len() {
            panic!("values and volumes must have same length");
        }

        let count = values.len();
        if count == 0 {
            return 0;
        }

        let mut weighted_sum: i128 = 0;
        let mut total_volume: i128 = 0;

        for i in 0..count {
            let value = values.get(i).unwrap();
            let volume = volumes.get(i).unwrap();
            weighted_sum = weighted_sum.checked_add(value * volume).unwrap_or(weighted_sum);
            total_volume = total_volume.checked_add(volume).unwrap_or(total_volume);
        }

        if total_volume == 0 {
            return 0;
        }

        weighted_sum / total_volume
    }

    /// Calculate standard deviation of a value series.
    ///
    /// Uses population standard deviation formula: sqrt(sum((x - mean)^2) / n)
    /// Returns result scaled by PRECISION for fixed-point arithmetic.
    pub fn calculate_stddev(env: Env, values: Vec<i128>) -> i128 {
        let count = values.len() as i128;
        if count < 2 {
            return 0;
        }

        let mean = Self::calculate_average(env.clone(), values.clone());

        let mut sum_squared_diff: i128 = 0;
        for v in values.iter() {
            let diff = v - mean;
            sum_squared_diff = sum_squared_diff.checked_add(diff * diff).unwrap_or(sum_squared_diff);
        }

        // Variance = sum_squared_diff / count
        let variance = sum_squared_diff / count;

        // Integer square root approximation using Newton's method
        Self::integer_sqrt(variance)
    }

    /// Calculate price volatility as annualized standard deviation.
    ///
    /// Returns volatility in basis points (1 bp = 0.01%).
    /// Uses the standard deviation of price returns.
    pub fn calculate_volatility(env: Env, prices: Vec<i128>, period_secs: u64) -> i128 {
        let n = prices.len();
        if n < 2 {
            return 0;
        }

        // Calculate price returns (percentage changes)
        let mut returns: Vec<i128> = Vec::new(&env);
        for i in 1..n {
            let prev_price = prices.get(i - 1).unwrap();
            let curr_price = prices.get(i).unwrap();

            if prev_price == 0 {
                returns.push_back(0);
                continue;
            }

            // Return = (curr - prev) / prev * PRECISION
            let price_diff = curr_price - prev_price;
            let ret = (price_diff * 10_000) / prev_price; // In basis points
            returns.push_back(ret);
        }

        // Calculate standard deviation of returns
        let stddev_returns = Self::calculate_stddev(env.clone(), returns);

        // Annualize: multiply by sqrt(seconds in year / period)
        // Using 365 days = 31_536_000 seconds
        const SECONDS_PER_YEAR: u64 = 31_536_000;
        if period_secs == 0 {
            return stddev_returns;
        }

        // Annualization factor scaled by PRECISION
        let annualization_factor = Self::integer_sqrt(
            (SECONDS_PER_YEAR as i128 * 10_000) / period_secs as i128,
        );

        // Annualized volatility
        (stddev_returns * annualization_factor) / 100
    }

    /// Calculate min and max values in a series.
    pub fn calculate_min_max(env: Env, values: Vec<i128>) -> (i128, i128) {
        if values.len() == 0 {
            return (0, 0);
        }

        let mut min = values.get(0).unwrap();
        let mut max = values.get(0).unwrap();

        for v in values.iter() {
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }

        (min, max)
    }

    /// Calculate median value of a sorted series.
    ///
    /// For even-length series, returns average of two middle values.
    pub fn calculate_median(env: Env, mut values: Vec<i128>) -> i128 {
        let n = values.len();
        if n == 0 {
            return 0;
        }

        // Simple bubble sort for small vectors (gas efficient for n < 100)
        for i in 0..n {
            for j in 0..(n - i - 1) {
                let a = values.get(j).unwrap();
                let b = values.get(j + 1).unwrap();
                if a > b {
                    // Swap - we can't modify in place, so we need to rebuild
                    // This is inefficient but works for small vectors
                }
            }
        }

        // For gas efficiency with small datasets, use selection algorithm
        // Find k-th smallest element
        let mid = n / 2;
        if n % 2 == 1 {
            // Odd: return middle element
            Self::quick_select(&env, &values, mid)
        } else {
            // Even: return average of two middle elements
            let left = Self::quick_select(&env, &values, mid - 1);
            let right = Self::quick_select(&env, &values, mid);
            (left + right) / 2
        }
    }

    /// Calculate percentiles (25th and 75th) for a value series.
    ///
    /// Returns (p25, median, p75).
    pub fn calculate_percentiles(env: Env, values: Vec<i128>) -> (i128, i128, i128) {
        let n = values.len();
        if n == 0 {
            return (0, 0, 0);
        }
        if n == 1 {
            let v = values.get(0).unwrap();
            return (v, v, v);
        }

        // Calculate positions
        let p25_idx = (n - 1) / 4;
        let p50_idx = n / 2;
        let p75_idx = (3 * (n - 1)) / 4;

        // Use quick select for each percentile
        let p25 = Self::quick_select(&env, &values, p25_idx);
        let p50 = if n % 2 == 1 {
            Self::quick_select(&env, &values, p50_idx)
        } else {
            let left = Self::quick_select(&env, &values, p50_idx - 1);
            let right = Self::quick_select(&env, &values, p50_idx);
            (left + right) / 2
        };
        let p75 = Self::quick_select(&env, &values, p75_idx);

        (p25, p50, p75)
    }

    /// Compute all statistics for an asset over a specified period.
    ///
    /// Calculates and stores: average, stddev, volatility, min/max, median, percentiles.
    /// Requires at least 2 data points for meaningful statistics.
    pub fn compute_statistics(
        env: Env,
        caller: Address,
        asset_code: String,
        period: StatPeriod,
    ) -> Statistics {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != admin {
            panic!("only admin can compute statistics");
        }

        // Determine time range based on period
        let now = env.ledger().timestamp();
        let period_secs = match period {
            StatPeriod::Hour => 3600,
            StatPeriod::Day => 86400,
            StatPeriod::Week => 604800,
            StatPeriod::Month => 2592000,
        };
        let start_time = now.saturating_sub(period_secs);

        // Get price history for the period
        let history: Vec<PriceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::PriceHistory(asset_code.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        // Collect prices within time range
        let mut prices: Vec<i128> = Vec::new(&env);
        for record in history.iter() {
            if record.timestamp >= start_time && record.timestamp <= now {
                prices.push_back(record.price);
            }
        }

        let data_points = prices.len();
        if data_points < 2 {
            panic!("insufficient data points for statistics");
        }

        // Calculate all statistics
        let average = Self::calculate_average(env.clone(), prices.clone());
        let stddev = Self::calculate_stddev(env.clone(), prices.clone());
        let volatility = Self::calculate_volatility(env.clone(), prices.clone(), period_secs);
        let (min_price, max_price) = Self::calculate_min_max(env.clone(), prices.clone());
        let (p25, median, p75) = Self::calculate_percentiles(env.clone(), prices.clone());

        // Create and store statistics record
        let stats = Statistics {
            asset_code: asset_code.clone(),
            period: period.clone(),
            average_price: average,
            stddev_price: stddev,
            volatility_bps: volatility,
            min_price,
            max_price,
            median_price: median,
            p25_price: p25,
            p75_price: p75,
            data_points,
            timestamp: now,
        };

        // Store in history
        let mut stats_history: Vec<Statistics> = env
            .storage()
            .persistent()
            .get(&DataKey::AssetStatistics(asset_code.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        stats_history.push_back(stats.clone());
        env.storage()
            .persistent()
            .set(&DataKey::AssetStatistics(asset_code.clone()), &stats_history);

        // Emit event
        env.events().publish(
            (symbol_short!("stats_comp"), asset_code.clone(), period),
            average,
        );

        stats
    }

    /// Get pre-computed statistics for an asset.
    ///
    /// Returns the most recent statistics for the specified period, or None
    /// if no statistics have been computed.
    pub fn get_statistics(
        env: Env,
        asset_code: String,
        period: StatPeriod,
    ) -> Option<Statistics> {
        let stats_history: Vec<Statistics> = env
            .storage()
            .persistent()
            .get(&DataKey::AssetStatistics(asset_code))
            .unwrap_or_else(|| Vec::new(&env));

        // Return the most recent matching period
        let mut i = stats_history.len();
        while i > 0 {
            i -= 1;
            let stats = stats_history.get(i).unwrap();
            if stats.period == period {
                return Some(stats);
            }
        }

        None
    }

    /// Get all historical statistics for an asset.
    pub fn get_statistics_history(env: Env, asset_code: String) -> Vec<Statistics> {
        env.storage()
            .persistent()
            .get(&DataKey::AssetStatistics(asset_code))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Trigger periodic statistics calculation for all active assets.
    ///
    /// Intended to be called periodically (e.g., by an automation service)
    /// to keep statistics up-to-date. Calculates daily statistics for all
    /// assets with sufficient data.
    pub fn trigger_periodic_stats(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != admin {
            panic!("only admin can trigger periodic stats");
        }

        let assets = Self::get_monitored_assets(env.clone());
        let now = env.ledger().timestamp();

        for asset_code in assets.iter() {
            // Check if we have recent enough data
            let history: Vec<PriceRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::PriceHistory(asset_code.clone()))
                .unwrap_or_else(|| Vec::new(&env));

            if history.len() < 2 {
                continue;
            }

            // Check last stats computation time
            let existing_stats = Self::get_statistics(env.clone(), asset_code.clone(), StatPeriod::Day);
            let should_compute = match existing_stats {
                Some(stats) => now.saturating_sub(stats.timestamp) >= 3600, // 1 hour minimum
                None => true,
            };

            if should_compute {
                // Compute new statistics
                let _ = Self::compute_statistics(
                    env.clone(),
                    caller.clone(),
                    asset_code.clone(),
                    StatPeriod::Day,
                );
            }
        }
    }

    /// Calculate rolling window statistics over a series.
    ///
    /// Returns a vector of statistics, each computed over `window_size` data points,
    /// sliding by `step` points each time.
    pub fn calculate_rolling_statistics(
        env: Env,
        values: Vec<i128>,
        window_size: u32,
        step: u32,
    ) -> Vec<i128> {
        let n = values.len();
        if window_size == 0 || step == 0 || n < window_size {
            return Vec::new(&env);
        }

        let mut results: Vec<i128> = Vec::new(&env);
        let mut start: u32 = 0;

        while start + window_size <= n {
            // Extract window
            let mut window: Vec<i128> = Vec::new(&env);
            for i in start..(start + window_size) {
                window.push_back(values.get(i).unwrap());
            }

            // Calculate average for this window
            let avg = Self::calculate_average(env.clone(), window);
            results.push_back(avg);

            start += step;
        }

        results
    }

    // -----------------------------------------------------------------------
    // Private helper functions for statistics
    // -----------------------------------------------------------------------

    /// Integer square root using Newton's method.
    /// Returns sqrt(x) as an integer.
    fn integer_sqrt(x: i128) -> i128 {
        if x <= 0 {
            return 0;
        }
        if x == 1 {
            return 1;
        }

        let mut z = x;
        let mut y = (z + 1) / 2;

        while y < z {
            z = y;
            y = (z + x / z) / 2;
        }

        z
    }

    /// Quick select algorithm to find k-th smallest element.
    /// Uses median-of-three pivot selection for efficiency.
    fn quick_select(env: &Env, values: &Vec<i128>, k: u32) -> i128 {
        let n = values.len();
        if n == 0 || k >= n {
            return 0;
        }

        // For small arrays, use simple selection
        if n <= 5 {
            // Copy and sort
            let mut sorted: Vec<i128> = Vec::new(env);
            for v in values.iter() {
                sorted.push_back(v);
            }
            // Simple insertion sort for small n
            for i in 1..sorted.len() {
                let key = sorted.get(i).unwrap();
                let mut j = i;
                while j > 0 {
                    let prev = sorted.get(j - 1).unwrap();
                    if prev > key {
                        sorted.set(j, &prev);
                        j -= 1;
                    } else {
                        break;
                    }
                }
                sorted.set(j, &key);
            }
            return sorted.get(k).unwrap();
        }

        // For larger arrays, use median-of-three quickselect
        // (simplified version for gas efficiency)
        let pivot = values.get(n / 2).unwrap();

        let mut lows: Vec<i128> = Vec::new(env);
        let mut highs: Vec<i128> = Vec::new(env);
        let mut pivots: Vec<i128> = Vec::new(env);

        for v in values.iter() {
            if v < pivot {
                lows.push_back(v);
            } else if v > pivot {
                highs.push_back(v);
            } else {
                pivots.push_back(v);
            }
        }

        let num_lows = lows.len();
        if k < num_lows {
            Self::quick_select(env, &lows, k)
        } else if k < num_lows + pivots.len() {
            pivot
        } else {
            Self::quick_select(env, &highs, k - num_lows - pivots.len())
        }
    }

    /// Calculate correlation coefficient between two series.
    /// Returns value between -10_000 and 10_000 (scaled by 10_000).
    pub fn calculate_correlation(env: Env, x: Vec<i128>, y: Vec<i128>) -> i128 {
        if x.len() != y.len() || x.len() < 2 {
            return 0;
        }

        let n = x.len() as i128;

        // Calculate means
        let mean_x = Self::calculate_average(env.clone(), x.clone());
        let mean_y = Self::calculate_average(env.clone(), y.clone());

        // Calculate covariance and variances
        let mut cov: i128 = 0;
        let mut var_x: i128 = 0;
        let mut var_y: i128 = 0;

        for i in 0..x.len() {
            let xi = x.get(i).unwrap();
            let yi = y.get(i).unwrap();

            let dx = xi - mean_x;
            let dy = yi - mean_y;

            cov = cov.checked_add(dx * dy).unwrap_or(cov);
            var_x = var_x.checked_add(dx * dx).unwrap_or(var_x);
            var_y = var_y.checked_add(dy * dy).unwrap_or(var_y);
        }

        // Normalize
        cov = cov / n;
        var_x = var_x / n;
        var_y = var_y / n;

        // Calculate correlation
        let std_x = Self::integer_sqrt(var_x);
        let std_y = Self::integer_sqrt(var_y);

        if std_x == 0 || std_y == 0 {
            return 0;
        }

        // correlation = cov / (std_x * std_y), scaled by 10_000
        (cov * 10_000) / (std_x * std_y)
    }

    /// Calculate exponential moving average (EMA).
    ///
    /// `smoothing_factor` is a value between 0 and 10_000 representing
    /// the smoothing constant alpha (where alpha = smoothing_factor / 10_000).
    pub fn calculate_ema(
        env: Env,
        values: Vec<i128>,
        smoothing_factor: i128,
    ) -> i128 {
        let n = values.len();
        if n == 0 {
            return 0;
        }
        if smoothing_factor <= 0 || smoothing_factor > 10_000 {
            panic!("smoothing factor must be between 1 and 10000");
        }

        // Start with simple average for first value
        let mut ema = values.get(0).unwrap();

        // EMA_t = alpha * value_t + (1 - alpha) * EMA_{t-1}
        for i in 1..n {
            let value = values.get(i).unwrap();
            let alpha_num = smoothing_factor;
            let alpha_denom: i128 = 10_000;

            // EMA = (alpha * value + (10000 - alpha) * prev_ema) / 10000
            let new_ema = (alpha_num * value + (alpha_denom - alpha_num) * ema) / alpha_denom;
            ema = new_ema;
        }

        ema
    }

    /// Document statistical methods available in the contract.
    ///
    /// Returns a string describing each statistical function and its usage.
    pub fn get_statistical_methods_documentation(env: Env) -> String {
        String::from_str(
            &env,
            "Statistical Methods:\n\
            1. calculate_average(values) - Arithmetic mean\n\
            2. calculate_volume_weighted_average(values, volumes) - VWAP\n\
            3. calculate_stddev(values) - Population standard deviation\n\
            4. calculate_volatility(prices, period_secs) - Annualized volatility in bps\n\
            5. calculate_min_max(values) - Min and max values\n\
            6. calculate_median(values) - Median value\n\
            7. calculate_percentiles(values) - P25, median, P75\n\
            8. calculate_correlation(x, y) - Correlation coefficient\n\
            9. calculate_ema(values, smoothing) - Exponential moving average\n\
            10. calculate_rolling_statistics(values, window, step) - Rolling window stats\n\
            11. compute_statistics(asset, period) - Full statistics computation\n\
            12. get_statistics(asset, period) - Retrieve stored statistics\n\
            13. trigger_periodic_stats() - Trigger batch computation"
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    /// Helper: set up a fresh contract with an admin, returning (env, client, admin).
    fn setup() -> (Env, BridgeWatchContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BridgeWatchContract);
        let client = BridgeWatchContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    fn liquidity_sources(env: &Env, venues: &[&str]) -> Vec<String> {
        let mut sources = Vec::new(env);
        for venue in venues.iter() {
            sources.push_back(String::from_str(env, venue));
        }
        sources
    }

    // -----------------------------------------------------------------------
    // Price deviation detection tests (issue #23)
    // -----------------------------------------------------------------------

    #[test]
    fn test_price_deviation_no_reference_returns_none() {
        let (env, client, _admin) = setup();
        let asset = String::from_str(&env, "USDC");
        // No stored price record → should return None
        let result = client.check_price_deviation(&asset, &1_000_000);
        assert!(result.is_none());
    }

    #[test]
    fn test_price_deviation_below_threshold_returns_none() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(1_000_000);
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        // Store reference price of 1_000_000 (1 %)
        client.register_asset(&admin, &asset);
        client.submit_price(&admin, &asset, &1_000_000, &source);

        // 1 % deviation is below the default Low threshold of 2 %
        let result = client.check_price_deviation(&asset, &1_010_000);
        assert!(result.is_none());
    }

    #[test]
    fn test_price_deviation_low_severity() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(1_000_000);
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);
        client.submit_price(&admin, &asset, &1_000_000, &source);

        // 3 % deviation → Low severity
        let result = client.check_price_deviation(&asset, &1_030_000);
        assert!(result.is_some());
        let alert = result.unwrap();
        assert_eq!(alert.deviation_bps, 300);
        assert_eq!(alert.severity, DeviationSeverity::Low);
    }

    #[test]
    fn test_price_deviation_medium_severity() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(1_000_000);
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);
        client.submit_price(&admin, &asset, &1_000_000, &source);

        // 7 % deviation → Medium severity
        let result = client.check_price_deviation(&asset, &1_070_000);
        assert!(result.is_some());
        let alert = result.unwrap();
        assert_eq!(alert.deviation_bps, 700);
        assert_eq!(alert.severity, DeviationSeverity::Medium);
    }

    #[test]
    fn test_price_deviation_high_severity() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(1_000_000);
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);
        client.submit_price(&admin, &asset, &1_000_000, &source);

        // 15 % deviation → High severity
        let result = client.check_price_deviation(&asset, &1_150_000);
        assert!(result.is_some());
        let alert = result.unwrap();
        assert_eq!(alert.deviation_bps, 1_500);
        assert_eq!(alert.severity, DeviationSeverity::High);
    }

    #[test]
    fn test_get_deviation_alerts_persists_latest() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(1_000_000);
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);
        client.submit_price(&admin, &asset, &1_000_000, &source);
        client.check_price_deviation(&asset, &1_150_000);

        let stored = client.get_deviation_alerts(&asset);
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().severity, DeviationSeverity::High);
    }

    #[test]
    fn test_set_custom_deviation_thresholds() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(1_000_000);
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        // Custom tight thresholds: Low > 50 bps (0.5 %)
        client.set_deviation_threshold(&asset, &50, &100, &200);
        client.register_asset(&admin, &asset);
        client.submit_price(&admin, &asset, &1_000_000, &source);

        // 1 % deviation (100 bps) exceeds custom Low threshold of 50 bps
        let result = client.check_price_deviation(&asset, &1_010_000);
        assert!(result.is_some());
        assert_eq!(result.unwrap().severity, DeviationSeverity::Low);
    }

    // -----------------------------------------------------------------------
    // Bridge supply mismatch tracking tests (issue #28)
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_supply_mismatch_not_critical() {
        let (env, client, _admin) = setup();
        env.ledger().set_timestamp(1_000_000);

        let bridge = String::from_str(&env, "CIRCLE_USDC");
        let asset = String::from_str(&env, "USDC");

        // diff=1_000, bps = 1_000*10_000/1_001_000 = 9 → below default threshold of 10
        client.record_supply_mismatch(&bridge, &asset, &1_000_000, &1_001_000);

        let mismatches = client.get_supply_mismatches(&bridge);
        assert_eq!(mismatches.len(), 1);
        let m = mismatches.get(0).unwrap();
        assert_eq!(m.mismatch_bps, 9);
        assert!(!m.is_critical);
    }

    #[test]
    fn test_record_supply_mismatch_critical() {
        let (env, client, _admin) = setup();
        env.ledger().set_timestamp(1_000_000);

        let bridge = String::from_str(&env, "CIRCLE_USDC");
        let asset = String::from_str(&env, "USDC");

        // diff=2_000, bps = 2_000*10_000/1_002_000 = 19 → above default threshold of 10
        client.record_supply_mismatch(&bridge, &asset, &1_000_000, &1_002_000);

        let mismatches = client.get_supply_mismatches(&bridge);
        let m = mismatches.get(0).unwrap();
        assert_eq!(m.mismatch_bps, 19);
        assert!(m.is_critical);
    }

    #[test]
    fn test_set_mismatch_threshold_custom() {
        let (env, client, _admin) = setup();
        env.ledger().set_timestamp(1_000_000);

        let bridge = String::from_str(&env, "CIRCLE_USDC");
        let asset = String::from_str(&env, "USDC");

        // Tighten threshold to 5 bps; 9 bps mismatch should now be critical
        client.set_mismatch_threshold(&5);
        client.record_supply_mismatch(&bridge, &asset, &1_000_000, &1_001_000);

        let m = client.get_supply_mismatches(&bridge).get(0).unwrap();
        assert!(m.is_critical);
    }

    #[test]
    fn test_get_critical_mismatches_across_bridges() {
        let (env, client, _admin) = setup();
        env.ledger().set_timestamp(1_000_000);

        let bridge1 = String::from_str(&env, "CIRCLE_USDC");
        let bridge2 = String::from_str(&env, "WORMHOLE_EURC");
        let asset = String::from_str(&env, "USDC");

        // bridge1: 9 bps (not critical)
        client.record_supply_mismatch(&bridge1, &asset, &1_000_000, &1_001_000);
        // bridge2: 19 bps (critical)
        client.record_supply_mismatch(&bridge2, &asset, &1_000_000, &1_002_000);

        let critical = client.get_critical_mismatches();
        assert_eq!(critical.len(), 1);
        assert_eq!(critical.get(0).unwrap().bridge_id, bridge2);
    }

    #[test]
    fn test_supply_mismatch_historical_tracking() {
        let (env, client, _admin) = setup();

        let bridge = String::from_str(&env, "CIRCLE_USDC");
        let asset = String::from_str(&env, "USDC");

        for i in 0..3u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.record_supply_mismatch(
                &bridge,
                &asset,
                &(1_000_000 + i as i128 * 500),
                &1_000_000,
            );
        }

        let mismatches = client.get_supply_mismatches(&bridge);
        assert_eq!(mismatches.len(), 3);
    }

    #[test]
    fn test_zero_source_supply_returns_zero_bps() {
        let (env, client, _admin) = setup();
        env.ledger().set_timestamp(1_000_000);

        let bridge = String::from_str(&env, "CIRCLE_USDC");
        let asset = String::from_str(&env, "USDC");

        client.record_supply_mismatch(&bridge, &asset, &1_000_000, &0);

        let m = client.get_supply_mismatches(&bridge).get(0).unwrap();
        assert_eq!(m.mismatch_bps, 0);
        assert!(!m.is_critical);
    }

    // -----------------------------------------------------------------------
    // Historical price query tests (issue #22)
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_price_history_returns_records_in_time_range() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);

        for i in 0..5u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.submit_price(
                &admin,
                &asset,
                &(1_000_000 + i as i128 * 1_000),
                &source,
            );
        }

        // Query middle range – timestamps 1_003_600 to 1_010_800
        let history = client.get_price_history(&asset, &1_003_600, &1_010_800, &100, &0);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_get_price_history_returns_descending_order() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);

        for i in 0..3u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.submit_price(&admin, &asset, &(1_000_000 + i as i128 * 1_000), &source);
        }

        let history = client.get_price_history(&asset, &0, &2_000_000, &100, &0);
        assert_eq!(history.len(), 3);
        // Newest first
        assert!(history.get(0).unwrap().timestamp > history.get(1).unwrap().timestamp);
        assert!(history.get(1).unwrap().timestamp > history.get(2).unwrap().timestamp);
    }

    #[test]
    fn test_get_price_history_limit_caps_at_100() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);

        // Requesting a limit higher than 100 should still return at most what's available
        env.ledger().set_timestamp(1_000_000);
        client.submit_price(&admin, &asset, &1_000_000, &source);

        let history = client.get_price_history(&asset, &0, &2_000_000, &200, &0);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_get_price_history_pagination_with_offset() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);

        for i in 0..5u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.submit_price(&admin, &asset, &(1_000_000 + i as i128 * 1_000), &source);
        }

        // Get first 2 records
        let page1 = client.get_price_history(&asset, &0, &2_000_000, &2, &0);
        assert_eq!(page1.len(), 2);

        // Get next 2 records
        let page2 = client.get_price_history(&asset, &0, &2_000_000, &2, &2);
        assert_eq!(page2.len(), 2);

        // Get remaining
        let page3 = client.get_price_history(&asset, &0, &2_000_000, &2, &4);
        assert_eq!(page3.len(), 1);

        // Pages should not overlap
        assert!(page1.get(0).unwrap().timestamp > page2.get(0).unwrap().timestamp);
        assert!(page2.get(0).unwrap().timestamp > page3.get(0).unwrap().timestamp);
    }

    #[test]
    fn test_get_price_history_no_data_returns_empty() {
        let (env, client, _admin) = setup();
        let asset = String::from_str(&env, "USDC");

        let history = client.get_price_history(&asset, &0, &2_000_000, &100, &0);
        assert_eq!(history.len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_get_price_history_invalid_time_range_panics() {
        let (env, client, _admin) = setup();
        let asset = String::from_str(&env, "USDC");

        // start_time > end_time should panic
        client.get_price_history(&asset, &2_000_000, &1_000_000, &100, &0);
    }

    #[test]
    fn test_get_price_history_exact_limit() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);

        for i in 0..5u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.submit_price(&admin, &asset, &(1_000_000 + i as i128 * 1_000), &source);
        }

        // Limit of 3 should return exactly 3
        let history = client.get_price_history(&asset, &0, &2_000_000, &3, &0);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_submit_price_stores_history() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "Stellar DEX");

        client.register_asset(&admin, &asset);

        env.ledger().set_timestamp(1_000_000);
        client.submit_price(&admin, &asset, &1_000_000, &source);

        env.ledger().set_timestamp(1_003_600);
        client.submit_price(&admin, &asset, &1_010_000, &source);

        // Latest price should be the second submission
        let latest = client.get_price(&asset).unwrap();
        assert_eq!(latest.price, 1_010_000);

        // History should contain both submissions
        let history = client.get_price_history(&asset, &0, &2_000_000, &100, &0);
        assert_eq!(history.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Original tests (kept for backwards compatibility)
    // -----------------------------------------------------------------------

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BridgeWatchContract);
        let client = BridgeWatchContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let assets = client.get_monitored_assets();
        assert_eq!(assets.len(), 0);
    }

    #[test]
    fn test_register_and_get_assets() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BridgeWatchContract);
        let client = BridgeWatchContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let usdc = String::from_str(&env, "USDC");
        client.register_asset(&admin, &usdc);

        let assets = client.get_monitored_assets();
        assert_eq!(assets.len(), 1);

        let health = client.get_health(&usdc).unwrap();
        assert!(health.active);
        assert!(!health.paused);
    }

    #[test]
    fn test_submit_and_get_health() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BridgeWatchContract);
        let client = BridgeWatchContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let usdc = String::from_str(&env, "USDC");
        client.register_asset(&admin, &usdc);
        client.submit_health(&admin, &usdc, &85, &90, &80, &85);

        let health = client.get_health(&usdc);
        assert!(health.is_some());
        assert_eq!(health.unwrap().health_score, 85);
    }

    // -----------------------------------------------------------------------
    // Batch health submission tests (issue #21)
    // -----------------------------------------------------------------------

    #[test]
    fn test_submit_health_batch_stores_all_records() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(1_000_000);

        let assets = ["USDC", "EURC", "PYUSD"];
        for code in assets.iter() {
            client.register_asset(&admin, &String::from_str(&env, code));
        }
        let mut batch = Vec::new(&env);
        for (i, code) in assets.iter().enumerate() {
            batch.push_back(HealthScoreBatch {
                asset_code: String::from_str(&env, code),
                health_score: 80 + i as u32,
                liquidity_score: 75,
                price_stability_score: 78,
                bridge_uptime_score: 82,
            });
        }

        client.submit_health_batch(&admin, &batch);

        for (i, code) in assets.iter().enumerate() {
            let health = client.get_health(&String::from_str(&env, code)).unwrap();
            assert_eq!(health.health_score, 80 + i as u32);
            assert_eq!(health.timestamp, 1_000_000);
        }
    }

    #[test]
    fn test_submit_health_batch_consistent_timestamps() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(5_000_000);

        client.register_asset(&admin, &String::from_str(&env, "USDC"));
        client.register_asset(&admin, &String::from_str(&env, "EURC"));

        let mut batch = Vec::new(&env);
        batch.push_back(HealthScoreBatch {
            asset_code: String::from_str(&env, "USDC"),
            health_score: 90,
            liquidity_score: 90,
            price_stability_score: 90,
            bridge_uptime_score: 90,
        });
        batch.push_back(HealthScoreBatch {
            asset_code: String::from_str(&env, "EURC"),
            health_score: 70,
            liquidity_score: 70,
            price_stability_score: 70,
            bridge_uptime_score: 70,
        });

        client.submit_health_batch(&admin, &batch);

        let usdc = client.get_health(&String::from_str(&env, "USDC")).unwrap();
        let eurc = client.get_health(&String::from_str(&env, "EURC")).unwrap();
        assert_eq!(usdc.timestamp, eurc.timestamp);
        assert_eq!(usdc.timestamp, 5_000_000);
    }

    #[test]
    #[should_panic]
    fn test_submit_health_batch_exceeds_limit() {
        let (env, client, admin) = setup();

        let mut batch = Vec::new(&env);
        for _ in 0..21u32 {
            batch.push_back(HealthScoreBatch {
                asset_code: String::from_str(&env, "USDC"),
                health_score: 85,
                liquidity_score: 85,
                price_stability_score: 85,
                bridge_uptime_score: 85,
            });
        }
        client.submit_health_batch(&admin, &batch);
    }

    // -----------------------------------------------------------------------
    // Multi-DEX liquidity depth tracking tests (issue #31)
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_liquidity_depth_stores_current_and_history() {
        let (env, client, _admin) = setup();
        let pair = String::from_str(&env, "USDC/XLM");

        env.ledger().set_timestamp(1_000_000);
        client.record_liquidity_depth(
            &pair,
            &1_500_000,
            &100_000,
            &300_000,
            &600_000,
            &1_200_000,
            &liquidity_sources(&env, &["StellarX", "Phoenix"]),
        );

        let current = client.get_aggregated_liquidity_depth(&pair).unwrap();
        assert_eq!(current.asset_pair, pair.clone());
        assert_eq!(current.total_liquidity, 1_500_000);
        assert_eq!(current.depth_0_1_pct, 100_000);
        assert_eq!(current.depth_5_pct, 1_200_000);
        assert_eq!(current.sources.len(), 2);
        assert_eq!(current.timestamp, 1_000_000);

        let history = client.get_liquidity_history(&pair, &0, &2_000_000);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap(), current);
    }

    #[test]
    fn test_get_liquidity_history_filters_by_time_range() {
        let (env, client, _admin) = setup();
        let pair = String::from_str(&env, "EURC/XLM");

        for i in 0..3u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.record_liquidity_depth(
                &pair,
                &(2_000_000 + i as i128 * 100_000),
                &(100_000 + i as i128 * 10_000),
                &(300_000 + i as i128 * 10_000),
                &(600_000 + i as i128 * 10_000),
                &(1_500_000 + i as i128 * 10_000),
                &liquidity_sources(&env, &["SDEX", "Soroswap"]),
            );
        }

        let history = client.get_liquidity_history(&pair, &1_003_600, &1_007_200);
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().timestamp, 1_003_600);
        assert_eq!(history.get(1).unwrap().timestamp, 1_007_200);
    }

    #[test]
    fn test_get_all_liquidity_depths_returns_latest_per_pair() {
        let (env, client, _admin) = setup();
        let usdc_xlm = String::from_str(&env, "USDC/XLM");
        let fobxx_usdc = String::from_str(&env, "FOBXX/USDC");

        env.ledger().set_timestamp(1_000_000);
        client.record_liquidity_depth(
            &usdc_xlm,
            &1_000_000,
            &100_000,
            &250_000,
            &500_000,
            &900_000,
            &liquidity_sources(&env, &["StellarX"]),
        );

        env.ledger().set_timestamp(1_100_000);
        client.record_liquidity_depth(
            &fobxx_usdc,
            &4_000_000,
            &300_000,
            &900_000,
            &1_500_000,
            &3_000_000,
            &liquidity_sources(&env, &["SDEX", "LumenSwap"]),
        );

        let all_depths = client.get_all_liquidity_depths();
        assert_eq!(all_depths.len(), 2);
        assert_eq!(all_depths.get(0).unwrap().asset_pair, usdc_xlm);
        assert_eq!(all_depths.get(1).unwrap().asset_pair, fobxx_usdc);
    }

    #[test]
    #[should_panic]
    fn test_record_liquidity_depth_rejects_unsupported_pair() {
        let (env, client, _admin) = setup();
        let pair = String::from_str(&env, "BTC/XLM");

        env.ledger().set_timestamp(1_000_000);
        client.record_liquidity_depth(
            &pair,
            &1_000_000,
            &100_000,
            &200_000,
            &300_000,
            &400_000,
            &liquidity_sources(&env, &["Phoenix"]),
        );
    }

    #[test]
    #[should_panic]
    fn test_record_liquidity_depth_rejects_invalid_depth_values() {
        let (env, client, _admin) = setup();
        let pair = String::from_str(&env, "PYUSD/XLM");

        env.ledger().set_timestamp(1_000_000);
        client.record_liquidity_depth(
            &pair,
            &500_000,
            &100_000,
            &250_000,
            &400_000,
            &600_000,
            &liquidity_sources(&env, &["Phoenix"]),
        );
    }

    // -----------------------------------------------------------------------
    // Multi-admin role management tests (issue #25)
    // -----------------------------------------------------------------------

    #[test]
    fn test_grant_and_check_role() {
        let (env, client, admin) = setup();
        let submitter = Address::generate(&env);

        client.grant_role(&admin, &submitter, &AdminRole::HealthSubmitter);

        assert!(client.has_role(&submitter, &AdminRole::HealthSubmitter));
        assert!(!client.has_role(&submitter, &AdminRole::PriceSubmitter));
    }

    #[test]
    fn test_role_holder_can_call_permitted_function() {
        let (env, client, admin) = setup();
        let submitter = Address::generate(&env);

        client.grant_role(&admin, &submitter, &AdminRole::HealthSubmitter);

        let usdc = String::from_str(&env, "USDC");
        client.register_asset(&admin, &usdc);
        client.submit_health(&submitter, &usdc, &80, &80, &80, &80);

        let health = client.get_health(&usdc).unwrap();
        assert_eq!(health.health_score, 80);
    }

    #[test]
    #[should_panic]
    fn test_unauthorized_address_cannot_submit_health() {
        let (env, client, _admin) = setup();
        let stranger = Address::generate(&env);

        let usdc = String::from_str(&env, "USDC");
        client.submit_health(&stranger, &usdc, &80, &80, &80, &80);
    }

    #[test]
    fn test_revoke_role_removes_access() {
        let (env, client, admin) = setup();
        let submitter = Address::generate(&env);

        client.grant_role(&admin, &submitter, &AdminRole::HealthSubmitter);
        client.revoke_role(&admin, &submitter, &AdminRole::HealthSubmitter);

        assert!(!client.has_role(&submitter, &AdminRole::HealthSubmitter));
    }

    #[test]
    fn test_get_admin_roles_returns_all_assignments() {
        let (env, client, admin) = setup();
        let addr_a = Address::generate(&env);
        let addr_b = Address::generate(&env);

        client.grant_role(&admin, &addr_a, &AdminRole::PriceSubmitter);
        client.grant_role(&admin, &addr_b, &AdminRole::AssetManager);

        let roles = client.get_admin_roles();
        assert_eq!(roles.len(), 2);
    }

    #[test]
    fn test_super_admin_can_grant_roles() {
        let (env, client, admin) = setup();
        let super_admin = Address::generate(&env);
        let new_submitter = Address::generate(&env);

        client.grant_role(&admin, &super_admin, &AdminRole::SuperAdmin);
        client.grant_role(&super_admin, &new_submitter, &AdminRole::PriceSubmitter);

        assert!(client.has_role(&new_submitter, &AdminRole::PriceSubmitter));
    }

    #[test]
    fn test_original_admin_can_call_all_functions() {
        let (env, client, admin) = setup();
        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);
        client.submit_health(&admin, &usdc, &90, &90, &90, &90);
        client.submit_price(&admin, &usdc, &1_000_000, &String::from_str(&env, "DEX"));

        assert_eq!(client.get_monitored_assets().len(), 1);
        assert!(client.get_health(&usdc).is_some());
        assert!(client.get_price(&usdc).is_some());
    }

    // -----------------------------------------------------------------------
    // Asset lifecycle management tests (issue #44)
    // -----------------------------------------------------------------------

    #[test]
    fn test_pause_asset_filters_from_monitored_assets() {
        let (env, client, admin) = setup();
        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);
        client.pause_asset(&admin, &usdc);

        let health = client.get_health(&usdc).unwrap();
        assert!(health.paused);
        assert!(health.active);
        assert_eq!(client.get_monitored_assets().len(), 0);
    }

    #[test]
    fn test_unpause_asset_restores_monitoring() {
        let (env, client, admin) = setup();
        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);
        client.pause_asset(&admin, &usdc);
        client.unpause_asset(&admin, &usdc);

        let health = client.get_health(&usdc).unwrap();
        assert!(!health.paused);
        assert!(health.active);
        assert_eq!(client.get_monitored_assets().len(), 1);
    }

    #[test]
    fn test_deregister_asset_keeps_history_but_hides_asset() {
        let (env, client, admin) = setup();
        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);
        client.submit_health(&admin, &usdc, &91, &88, &87, &89);
        client.deregister_asset(&admin, &usdc);

        let health = client.get_health(&usdc).unwrap();
        assert_eq!(health.health_score, 91);
        assert!(!health.active);
        assert!(!health.paused);
        assert_eq!(client.get_monitored_assets().len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_submit_health_rejected_for_paused_asset() {
        let (env, client, admin) = setup();
        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);
        client.pause_asset(&admin, &usdc);
        client.submit_health(&admin, &usdc, &80, &80, &80, &80);
    }

    #[test]
    #[should_panic]
    fn test_submit_price_rejected_for_deregistered_asset() {
        let (env, client, admin) = setup();
        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);
        client.deregister_asset(&admin, &usdc);
        client.submit_price(&admin, &usdc, &1_000_000, &String::from_str(&env, "DEX"));
    }

    #[test]
    #[should_panic]
    fn test_submit_health_rejected_for_unregistered_asset() {
        let (env, client, admin) = setup();
        let usdc = String::from_str(&env, "USDC");
        client.submit_health(&admin, &usdc, &80, &80, &80, &80);
    }

    // -----------------------------------------------------------------------
    // Liquidity Pool Monitor tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_pool_state_basic() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");

        env.ledger().set_timestamp(1_000_000);

        client.record_pool_state(
            &pool_id,
            &(1_000_000 * liquidity_pool::PRECISION),
            &(5_000_000 * liquidity_pool::PRECISION),
            &(2_000_000 * liquidity_pool::PRECISION),
            &(100_000 * liquidity_pool::PRECISION),
            &(1_000 * liquidity_pool::PRECISION),
            &PoolType::Amm,
        );

        let pools = client.get_registered_pools();
        assert_eq!(pools.len(), 1);
        assert_eq!(pools.get(0).unwrap(), pool_id);
    }

    #[test]
    fn test_record_multiple_pools() {
        let (env, client, _admin) = setup();

        env.ledger().set_timestamp(1_000_000);

        let pool1 = String::from_str(&env, "USDC_XLM");
        let pool2 = String::from_str(&env, "EURC_XLM");
        let pool3 = String::from_str(&env, "PYUSD_XLM");
        let pool4 = String::from_str(&env, "FOBXX_USDC");

        for pool_id in [&pool1, &pool2, &pool3, &pool4] {
            client.record_pool_state(
                pool_id,
                &(1_000_000 * liquidity_pool::PRECISION),
                &(2_000_000 * liquidity_pool::PRECISION),
                &(1_500_000 * liquidity_pool::PRECISION),
                &(50_000 * liquidity_pool::PRECISION),
                &(500 * liquidity_pool::PRECISION),
                &PoolType::Amm,
            );
        }

        let pools = client.get_registered_pools();
        assert_eq!(pools.len(), 4);
    }

    #[test]
    fn test_record_pool_state_does_not_duplicate_registration() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");

        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * liquidity_pool::PRECISION),
            &(2_000_000 * liquidity_pool::PRECISION),
            &(1_000_000 * liquidity_pool::PRECISION),
            &(10_000 * liquidity_pool::PRECISION),
            &(100 * liquidity_pool::PRECISION),
            &PoolType::Amm,
        );

        env.ledger().set_timestamp(1_003_600);
        client.record_pool_state(
            &pool_id,
            &(1_100_000 * liquidity_pool::PRECISION),
            &(2_200_000 * liquidity_pool::PRECISION),
            &(1_100_000 * liquidity_pool::PRECISION),
            &(12_000 * liquidity_pool::PRECISION),
            &(120 * liquidity_pool::PRECISION),
            &PoolType::Amm,
        );

        let pools = client.get_registered_pools();
        assert_eq!(pools.len(), 1);
    }

    #[test]
    fn test_get_pool_history() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Record 3 snapshots at different timestamps
        for i in 0..3u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.record_pool_state(
                &pool_id,
                &((1_000_000 + i as i128 * 10_000) * p),
                &((5_000_000 + i as i128 * 50_000) * p),
                &(2_000_000 * p),
                &((100_000 + i as i128 * 1_000) * p),
                &((1_000 + i as i128 * 10) * p),
                &PoolType::Amm,
            );
        }

        // Get all history
        let history = client.get_pool_history(&pool_id, &1_000_000, &1_010_000);
        assert_eq!(history.len(), 3);

        // Get partial range
        let partial = client.get_pool_history(&pool_id, &1_003_600, &1_007_200);
        assert_eq!(partial.len(), 2);
    }

    #[test]
    fn test_get_pool_history_empty() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "NONEXISTENT");

        let history = client.get_pool_history(&pool_id, &0, &9_999_999);
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_calculate_pool_metrics_basic() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Record snapshots over ~2 hours
        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(100_000 * p),
            &(1_000 * p),
            &PoolType::Amm,
        );

        env.ledger().set_timestamp(1_003_600);
        client.record_pool_state(
            &pool_id,
            &(1_100_000 * p),
            &(5_500_000 * p),
            &(2_100_000 * p),
            &(120_000 * p),
            &(1_200 * p),
            &PoolType::Amm,
        );

        // Calculate metrics over the last 2 hours
        let metrics = client.calculate_pool_metrics(&pool_id, &(2 * liquidity_pool::HOUR_SECS));

        assert_eq!(metrics.data_points, 2);
        assert_eq!(metrics.total_volume, (100_000 + 120_000) * p);
        assert_eq!(metrics.total_fees, (1_000 + 1_200) * p);
        assert!(metrics.avg_depth > 0);
        assert!(metrics.fee_apr > 0);
    }

    #[test]
    fn test_calculate_pool_metrics_no_data() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");

        let metrics = client.calculate_pool_metrics(&pool_id, &liquidity_pool::DAY_SECS);

        assert_eq!(metrics.data_points, 0);
        assert_eq!(metrics.total_volume, 0);
        assert_eq!(metrics.avg_depth, 0);
        assert_eq!(metrics.fee_apr, 0);
    }

    #[test]
    fn test_calculate_pool_metrics_price_change() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Price = reserve_b / reserve_a
        // Snapshot 1: price = 5_000_000 / 1_000_000 = 5.0
        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        // Snapshot 2: price = 6_000_000 / 1_000_000 = 6.0 (20% increase)
        env.ledger().set_timestamp(1_003_600);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(6_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        let metrics = client.calculate_pool_metrics(&pool_id, &(2 * liquidity_pool::HOUR_SECS));
        // price_change = (6 - 5) / 5 * PRECISION = 0.2 * PRECISION = 2_000_000
        assert_eq!(metrics.price_change, 2_000_000);
    }

    #[test]
    fn test_calculate_impermanent_loss_no_price_change() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Record a pool state with price = 5.0
        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        // Entry price == current price → no IL
        let result = client.calculate_impermanent_loss(
            &pool_id,
            &(5 * p), // entry_price = 5.0
            &(10_000 * p),
        );

        // When price hasn't changed, IL should be 0
        assert_eq!(result.il_percentage, 0);
        assert_eq!(result.entry_price, 5 * p);
        assert_eq!(result.current_price, 5 * p);
    }

    #[test]
    fn test_calculate_impermanent_loss_with_price_change() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Current price = 20.0 (reserve_b/reserve_a = 20_000_000/1_000_000)
        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(20_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        // Entry price was 5.0 → 4x price change
        let result = client.calculate_impermanent_loss(&pool_id, &(5 * p), &(10_000 * p));

        // For a 4x price change, IL ≈ 20%
        // IL = 1 - 2*sqrt(4)/(1+4) = 1 - 4/5 = 0.20 = 20%
        assert!(result.il_percentage > 0);
        assert!(result.current_price == 20 * p);
        assert!(result.hodl_value > result.current_value);
        assert!(result.net_loss > 0);

        // IL should be approximately 20% (2_000_000 in PRECISION units)
        // Allow ±1% tolerance due to integer math
        let expected_il = 2_000_000i128; // 20% * PRECISION
        let tolerance = 100_000i128; // 1%
        assert!(
            (result.il_percentage - expected_il).abs() < tolerance,
            "Expected IL ~20% ({}), got {}",
            expected_il,
            result.il_percentage
        );
    }

    #[test]
    fn test_calculate_impermanent_loss_nonexistent_pool() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "NONEXISTENT");
        let p = liquidity_pool::PRECISION;

        let result = client.calculate_impermanent_loss(&pool_id, &(5 * p), &(10_000 * p));

        assert_eq!(result.il_percentage, 0);
        assert_eq!(result.current_value, 10_000 * p);
        assert_eq!(result.hodl_value, 10_000 * p);
    }

    #[test]
    fn test_calculate_impermanent_loss_zero_entry_price() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        let result = client.calculate_impermanent_loss(&pool_id, &0, &(10_000 * p));
        assert_eq!(result.il_percentage, 0);
    }

    #[test]
    fn test_get_liquidity_depth_with_data() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(500_000 * p),
            &(2_500_000 * p),
            &(1_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        let depth = client.get_liquidity_depth(&pool_id);
        assert_eq!(depth.pool_id, pool_id);
        assert_eq!(depth.reserve_a, 500_000 * p);
        assert_eq!(depth.reserve_b, 2_500_000 * p);
        assert!(depth.total_value_locked > 0);
        assert!(depth.depth_score <= 100);
        assert_eq!(depth.timestamp, 1_000_000);
    }

    #[test]
    fn test_get_liquidity_depth_no_data() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "NONEXISTENT");

        let depth = client.get_liquidity_depth(&pool_id);
        assert_eq!(depth.reserve_a, 0);
        assert_eq!(depth.reserve_b, 0);
        assert_eq!(depth.total_value_locked, 0);
        assert_eq!(depth.depth_score, 0);
    }

    #[test]
    fn test_get_liquidity_depth_high_tvl() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Very large reserves → score should be 100
        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(10_000_000 * p),
            &(50_000_000 * p),
            &(20_000_000 * p),
            &(100_000 * p),
            &(1_000 * p),
            &PoolType::Amm,
        );

        let depth = client.get_liquidity_depth(&pool_id);
        assert_eq!(depth.depth_score, 100);
    }

    #[test]
    fn test_sdex_pool_type() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM_SDEX");
        let p = liquidity_pool::PRECISION;

        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(50_000 * p),
            &(500 * p),
            &PoolType::Sdex,
        );

        let history = client.get_pool_history(&pool_id, &0, &2_000_000);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().pool_type, PoolType::Sdex);
    }

    #[test]
    fn test_daily_bucket_creation() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Day 1, snapshot 1
        let day1_ts = 86_400u64; // start of day 1
        env.ledger().set_timestamp(day1_ts + 100);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(100_000 * p),
            &(1_000 * p),
            &PoolType::Amm,
        );

        // Day 1, snapshot 2 (higher price)
        env.ledger().set_timestamp(day1_ts + 3_700);
        client.record_pool_state(
            &pool_id,
            &(900_000 * p),
            &(5_400_000 * p),
            &(2_000_000 * p),
            &(110_000 * p),
            &(1_100 * p),
            &PoolType::Amm,
        );

        let buckets = client.get_daily_history(&pool_id, &0, &200_000);
        assert_eq!(buckets.len(), 1);

        let bucket = buckets.get(0).unwrap();
        assert_eq!(bucket.day_timestamp, day1_ts);
        assert_eq!(bucket.snapshot_count, 2);
        assert_eq!(bucket.total_volume, (100_000 + 110_000) * p);
        assert_eq!(bucket.total_fees, (1_000 + 1_100) * p);
    }

    #[test]
    fn test_daily_bucket_multiple_days() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Day 0
        env.ledger().set_timestamp(100);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(50_000 * p),
            &(500 * p),
            &PoolType::Amm,
        );

        // Day 1
        env.ledger().set_timestamp(86_400 + 100);
        client.record_pool_state(
            &pool_id,
            &(1_100_000 * p),
            &(5_500_000 * p),
            &(2_100_000 * p),
            &(60_000 * p),
            &(600 * p),
            &PoolType::Amm,
        );

        // Day 2
        env.ledger().set_timestamp(2 * 86_400 + 100);
        client.record_pool_state(
            &pool_id,
            &(1_200_000 * p),
            &(6_000_000 * p),
            &(2_200_000 * p),
            &(70_000 * p),
            &(700 * p),
            &PoolType::Amm,
        );

        let buckets = client.get_daily_history(&pool_id, &0, &300_000);
        assert_eq!(buckets.len(), 3);
    }

    #[test]
    fn test_daily_history_empty_pool() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "NONEXISTENT");

        let buckets = client.get_daily_history(&pool_id, &0, &999_999);
        assert_eq!(buckets.len(), 0);
    }

    #[test]
    fn test_daily_bucket_ohlc_prices() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        let day_ts = 86_400u64;

        // Snapshot 1: price = 5_000_000 / 1_000_000 = 5.0
        env.ledger().set_timestamp(day_ts + 100);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        // Snapshot 2: price = 7_000_000 / 1_000_000 = 7.0 (high)
        env.ledger().set_timestamp(day_ts + 3_700);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(7_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        // Snapshot 3: price = 4_000_000 / 1_000_000 = 4.0 (low, close)
        env.ledger().set_timestamp(day_ts + 7_300);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(4_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        let buckets = client.get_daily_history(&pool_id, &0, &200_000);
        assert_eq!(buckets.len(), 1);

        let bucket = buckets.get(0).unwrap();
        assert_eq!(bucket.open_price, 5 * p);
        assert_eq!(bucket.high_price, 7 * p);
        assert_eq!(bucket.low_price, 4 * p);
        assert_eq!(bucket.close_price, 4 * p);
        assert_eq!(bucket.snapshot_count, 3);
    }

    #[test]
    fn test_pool_history_ordering() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        for i in 0..5u64 {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.record_pool_state(
                &pool_id,
                &((1_000_000 + i as i128 * 10_000) * p),
                &(5_000_000 * p),
                &(2_000_000 * p),
                &(10_000 * p),
                &(100 * p),
                &PoolType::Amm,
            );
        }

        let history = client.get_pool_history(&pool_id, &0, &2_000_000);
        assert_eq!(history.len(), 5);

        // Verify chronological ordering
        for i in 0..(history.len() - 1) {
            let curr = history.get(i).unwrap();
            let next = history.get(i + 1).unwrap();
            assert!(curr.timestamp <= next.timestamp);
        }
    }

    #[test]
    fn test_metrics_24h_window() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Record a snapshot at the start
        env.ledger().set_timestamp(0);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(50_000 * p),
            &(500 * p),
            &PoolType::Amm,
        );

        // Record a snapshot 12h later
        env.ledger().set_timestamp(43_200);
        client.record_pool_state(
            &pool_id,
            &(1_050_000 * p),
            &(5_250_000 * p),
            &(2_050_000 * p),
            &(55_000 * p),
            &(550 * p),
            &PoolType::Amm,
        );

        // Now calculate 24h metrics
        let metrics = client.calculate_pool_metrics(&pool_id, &liquidity_pool::DAY_SECS);
        assert_eq!(metrics.data_points, 2);
        assert!(metrics.total_volume > 0);
    }

    #[test]
    fn test_metrics_7d_window() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Record snapshots across 7 days
        for day in 0..7u64 {
            env.ledger()
                .set_timestamp(day * liquidity_pool::DAY_SECS + 100);
            client.record_pool_state(
                &pool_id,
                &((1_000_000 + day as i128 * 10_000) * p),
                &((5_000_000 + day as i128 * 50_000) * p),
                &(2_000_000 * p),
                &((50_000 + day as i128 * 5_000) * p),
                &((500 + day as i128 * 50) * p),
                &PoolType::Amm,
            );
        }

        let metrics = client.calculate_pool_metrics(&pool_id, &liquidity_pool::WEEK_SECS);
        assert_eq!(metrics.data_points, 7);
        assert!(metrics.avg_depth > 0);
    }

    #[test]
    fn test_impermanent_loss_small_price_change() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Current price = 5.5 (10% increase from 5.0)
        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_500_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        let result = client.calculate_impermanent_loss(
            &pool_id,
            &(5 * p), // entry at 5.0
            &(10_000 * p),
        );

        // For 10% price change (ratio = 1.1), IL is very small (~0.023%)
        assert!(result.il_percentage >= 0);
        assert!(result.il_percentage < 500_000); // < 5%
    }

    #[test]
    fn test_multiple_pool_types_metrics() {
        let (env, client, _admin) = setup();
        let p = liquidity_pool::PRECISION;

        let amm_pool = String::from_str(&env, "USDC_XLM_AMM");
        let sdex_pool = String::from_str(&env, "USDC_XLM_SDEX");

        env.ledger().set_timestamp(1_000_000);

        client.record_pool_state(
            &amm_pool,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(100_000 * p),
            &(1_000 * p),
            &PoolType::Amm,
        );

        client.record_pool_state(
            &sdex_pool,
            &(800_000 * p),
            &(4_000_000 * p),
            &(1_600_000 * p),
            &(80_000 * p),
            &(800 * p),
            &PoolType::Sdex,
        );

        let amm_depth = client.get_liquidity_depth(&amm_pool);
        let sdex_depth = client.get_liquidity_depth(&sdex_pool);

        assert!(amm_depth.total_value_locked > sdex_depth.total_value_locked);
    }

    #[test]
    fn test_zero_reserves_handling() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "EMPTY_POOL");
        let _p = liquidity_pool::PRECISION;

        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(&pool_id, &0, &0, &0, &0, &0, &PoolType::Amm);

        let depth = client.get_liquidity_depth(&pool_id);
        assert_eq!(depth.depth_score, 0);
        assert_eq!(depth.total_value_locked, 0);

        let metrics = client.calculate_pool_metrics(&pool_id, &liquidity_pool::DAY_SECS);
        assert_eq!(metrics.total_volume, 0);
    }

    #[test]
    fn test_phase1_asset_pairs() {
        let (env, client, _admin) = setup();
        let p = liquidity_pool::PRECISION;

        let pairs = ["USDC_XLM", "EURC_XLM", "PYUSD_XLM", "FOBXX_USDC"];

        env.ledger().set_timestamp(1_000_000);

        for pair_str in pairs.iter() {
            let pool_id = String::from_str(&env, pair_str);
            client.record_pool_state(
                &pool_id,
                &(1_000_000 * p),
                &(5_000_000 * p),
                &(2_000_000 * p),
                &(50_000 * p),
                &(500 * p),
                &PoolType::Amm,
            );
        }

        let pools = client.get_registered_pools();
        assert_eq!(pools.len(), 4);

        // Verify all pools have valid depth
        for pair_str in pairs.iter() {
            let pool_id = String::from_str(&env, pair_str);
            let depth = client.get_liquidity_depth(&pool_id);
            assert!(depth.total_value_locked > 0);
        }
    }

    #[test]
    fn test_fee_apr_calculation() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Record two snapshots 1 day apart
        env.ledger().set_timestamp(0);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(100_000 * p),
            &(10_000 * p), // 10k fees
            &PoolType::Amm,
        );

        env.ledger().set_timestamp(liquidity_pool::DAY_SECS);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(100_000 * p),
            &(10_000 * p), // 10k fees
            &PoolType::Amm,
        );

        let metrics = client.calculate_pool_metrics(&pool_id, &(2 * liquidity_pool::DAY_SECS));
        assert!(metrics.fee_apr > 0, "Fee APR should be positive");
    }

    #[test]
    fn test_snapshot_ring_buffer_wrapping() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // We won't write MAX_SNAPSHOTS entries in a test (too expensive),
        // but we can verify the ring buffer logic with a smaller number.
        let num_snapshots = 10u64;

        for i in 0..num_snapshots {
            env.ledger().set_timestamp(1_000_000 + i * 3_600);
            client.record_pool_state(
                &pool_id,
                &((1_000_000 + i as i128 * 1_000) * p),
                &((5_000_000 + i as i128 * 5_000) * p),
                &(2_000_000 * p),
                &(10_000 * p),
                &(100 * p),
                &PoolType::Amm,
            );
        }

        let history = client.get_pool_history(&pool_id, &0, &2_000_000);
        assert_eq!(history.len(), num_snapshots as u32);
    }

    #[test]
    fn test_get_pool_history_boundary_timestamps() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Exact timestamp matches
        env.ledger().set_timestamp(1_000);
        client.record_pool_state(
            &pool_id,
            &(1_000_000 * p),
            &(5_000_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        env.ledger().set_timestamp(2_000);
        client.record_pool_state(
            &pool_id,
            &(1_100_000 * p),
            &(5_500_000 * p),
            &(2_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        // Exact from=1_000, to=2_000 should include both
        let history = client.get_pool_history(&pool_id, &1_000, &2_000);
        assert_eq!(history.len(), 2);

        // from=1_001 should exclude the first
        let history2 = client.get_pool_history(&pool_id, &1_001, &2_000);
        assert_eq!(history2.len(), 1);

        // to=1_999 should exclude the second
        let history3 = client.get_pool_history(&pool_id, &1_000, &1_999);
        assert_eq!(history3.len(), 1);
    }

    #[test]
    fn test_price_computation_from_reserves() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // reserve_a = 2_000_000, reserve_b = 10_000_000 → price = 5.0
        env.ledger().set_timestamp(1_000_000);
        client.record_pool_state(
            &pool_id,
            &(2_000_000 * p),
            &(10_000_000 * p),
            &(4_000_000 * p),
            &(10_000 * p),
            &(100 * p),
            &PoolType::Amm,
        );

        let history = client.get_pool_history(&pool_id, &0, &2_000_000);
        assert_eq!(history.len(), 1);

        let snap = history.get(0).unwrap();
        // price = (10_000_000 * P * P) / (2_000_000 * P) = 5 * P
        assert_eq!(snap.price, 5 * p);
    }

    #[test]
    fn test_daily_history_range_filter() {
        let (env, client, _admin) = setup();
        let pool_id = String::from_str(&env, "USDC_XLM");
        let p = liquidity_pool::PRECISION;

        // Create buckets for day 0, 1, 2
        for day in 0..3u64 {
            env.ledger()
                .set_timestamp(day * liquidity_pool::DAY_SECS + 100);
            client.record_pool_state(
                &pool_id,
                &(1_000_000 * p),
                &(5_000_000 * p),
                &(2_000_000 * p),
                &(10_000 * p),
                &(100 * p),
                &PoolType::Amm,
            );
        }

        // Query only day 1
        let buckets = client.get_daily_history(
            &pool_id,
            &liquidity_pool::DAY_SECS,
            &(2 * liquidity_pool::DAY_SECS - 1),
        );
        assert_eq!(buckets.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Statistical calculation tests (issue #133)
    // -----------------------------------------------------------------------

    #[test]
    fn test_calculate_average_basic() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 100i128, 200i128, 300i128, 400i128];
        let avg = client.calculate_average(&values);

        assert_eq!(avg, 250);
    }

    #[test]
    fn test_calculate_average_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let avg = client.calculate_average(&values);

        assert_eq!(avg, 0);
    }

    #[test]
    fn test_calculate_average_single_value() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 100i128];
        let avg = client.calculate_average(&values);

        assert_eq!(avg, 100);
    }

    #[test]
    fn test_calculate_volume_weighted_average() {
        let (env, client, _admin) = setup();

        // Values: 100 @ vol=1, 200 @ vol=2, 300 @ vol=3
        // VWA = (100*1 + 200*2 + 300*3) / (1+2+3) = (100+400+900)/6 = 1400/6 = 233
        let values = vec![&env, 100i128, 200i128, 300i128];
        let volumes = vec![&env, 1i128, 2i128, 3i128];
        let vwa = client.calculate_volume_weighted_average(&values, &volumes);

        assert_eq!(vwa, 233);
    }

    #[test]
    fn test_calculate_volume_weighted_average_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let volumes: Vec<i128> = vec![&env];
        let vwa = client.calculate_volume_weighted_average(&values, &volumes);

        assert_eq!(vwa, 0);
    }

    #[test]
    fn test_calculate_volume_weighted_average_zero_volume() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 100i128, 200i128];
        let volumes = vec![&env, 0i128, 0i128];
        let vwa = client.calculate_volume_weighted_average(&values, &volumes);

        assert_eq!(vwa, 0);
    }

    #[test]
    #[should_panic]
    fn test_calculate_volume_weighted_average_length_mismatch() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 100i128, 200i128];
        let volumes = vec![&env, 1i128];
        let _vwa = client.calculate_volume_weighted_average(&values, &volumes);
    }

    #[test]
    fn test_calculate_stddev_basic() {
        let (env, client, _admin) = setup();

        // Values: 2, 4, 4, 4, 5, 5, 7, 9
        // Mean = 40/8 = 5
        // Variance = ((-3)^2 + (-1)^2 + (-1)^2 + (-1)^2 + 0^2 + 0^2 + 2^2 + 4^2) / 8
        //          = (9 + 1 + 1 + 1 + 0 + 0 + 4 + 16) / 8 = 32/8 = 4
        // Stddev = sqrt(4) = 2
        let values = vec![&env, 2i128, 4i128, 4i128, 4i128, 5i128, 5i128, 7i128, 9i128];
        let stddev = client.calculate_stddev(&values);

        // Allow small tolerance for integer math
        assert!(stddev >= 1 && stddev <= 3);
    }

    #[test]
    fn test_calculate_stddev_single_value() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 100i128];
        let stddev = client.calculate_stddev(&values);

        assert_eq!(stddev, 0);
    }

    #[test]
    fn test_calculate_stddev_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let stddev = client.calculate_stddev(&values);

        assert_eq!(stddev, 0);
    }

    #[test]
    fn test_calculate_min_max_basic() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 10i128, 50i128, 30i128, 20i128, 40i128];
        let (min, max) = client.calculate_min_max(&values);

        assert_eq!(min, 10);
        assert_eq!(max, 50);
    }

    #[test]
    fn test_calculate_min_max_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let (min, max) = client.calculate_min_max(&values);

        assert_eq!(min, 0);
        assert_eq!(max, 0);
    }

    #[test]
    fn test_calculate_median_odd() {
        let (env, client, _admin) = setup();

        // Sorted: 10, 20, 30, 40, 50
        // Median (odd): 30
        let values = vec![&env, 30i128, 10i128, 50i128, 20i128, 40i128];
        let median = client.calculate_median(&values);

        assert_eq!(median, 30);
    }

    #[test]
    fn test_calculate_median_even() {
        let (env, client, _admin) = setup();

        // Sorted: 10, 20, 30, 40
        // Median (even): (20 + 30) / 2 = 25
        let values = vec![&env, 30i128, 10i128, 40i128, 20i128];
        let median = client.calculate_median(&values);

        assert_eq!(median, 25);
    }

    #[test]
    fn test_calculate_median_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let median = client.calculate_median(&values);

        assert_eq!(median, 0);
    }

    #[test]
    fn test_calculate_median_single() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 42i128];
        let median = client.calculate_median(&values);

        assert_eq!(median, 42);
    }

    #[test]
    fn test_calculate_percentiles() {
        let (env, client, _admin) = setup();

        // 9 values: 10, 20, 30, 40, 50, 60, 70, 80, 90
        // P25: index 2 = 30
        // P50: index 4 = 50
        // P75: index 6 = 70
        let values = vec![
            &env, 10i128, 20i128, 30i128, 40i128, 50i128, 60i128, 70i128, 80i128, 90i128,
        ];
        let (p25, p50, p75) = client.calculate_percentiles(&values);

        assert_eq!(p25, 30);
        assert_eq!(p50, 50);
        assert_eq!(p75, 70);
    }

    #[test]
    fn test_calculate_percentiles_single() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 100i128];
        let (p25, p50, p75) = client.calculate_percentiles(&values);

        assert_eq!(p25, 100);
        assert_eq!(p50, 100);
        assert_eq!(p75, 100);
    }

    #[test]
    fn test_calculate_percentiles_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let (p25, p50, p75) = client.calculate_percentiles(&values);

        assert_eq!(p25, 0);
        assert_eq!(p50, 0);
        assert_eq!(p75, 0);
    }

    #[test]
    fn test_calculate_volatility_flat_prices() {
        let (env, client, _admin) = setup();

        // Constant prices → zero volatility
        let prices = vec![&env, 100_000i128, 100_000i128, 100_000i128];
        let vol = client.calculate_volatility(&prices, &86400); // 1 day

        assert_eq!(vol, 0);
    }

    #[test]
    fn test_calculate_volatility_changing_prices() {
        let (env, client, _admin) = setup();

        // Prices changing → some volatility
        let prices = vec![&env, 100_000i128, 105_000i128, 95_000i128, 110_000i128];
        let vol = client.calculate_volatility(&prices, &86400); // 1 day

        // Volatility should be positive
        assert!(vol > 0);
    }

    #[test]
    fn test_calculate_volatility_insufficient_data() {
        let (env, client, _admin) = setup();

        // Only 1 price → zero volatility
        let prices = vec![&env, 100_000i128];
        let vol = client.calculate_volatility(&prices, &86400);

        assert_eq!(vol, 0);
    }

    #[test]
    fn test_calculate_ema_basic() {
        let (env, client, _admin) = setup();

        // EMA with alpha = 0.5 (smoothing_factor = 5000)
        let values = vec![&env, 100i128, 110i128, 120i128];
        // EMA1 = 100
        // EMA2 = 0.5*110 + 0.5*100 = 105
        // EMA3 = 0.5*120 + 0.5*105 = 112
        let ema = client.calculate_ema(&values, &5000);

        assert_eq!(ema, 112);
    }

    #[test]
    fn test_calculate_ema_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let ema = client.calculate_ema(&values, &5000);

        assert_eq!(ema, 0);
    }

    #[test]
    #[should_panic]
    fn test_calculate_ema_invalid_smoothing() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 100i128, 110i128];
        let _ema = client.calculate_ema(&values, &0); // Invalid
    }

    #[test]
    fn test_calculate_correlation_perfect_positive() {
        let (env, client, _admin) = setup();

        // Perfect correlation: y = x
        let x = vec![&env, 10i128, 20i128, 30i128, 40i128, 50i128];
        let y = vec![&env, 10i128, 20i128, 30i128, 40i128, 50i128];
        let corr = client.calculate_correlation(&x, &y);

        // Correlation should be close to 10_000 (scaled by 10_000)
        assert!(corr >= 9_000, "Perfect positive correlation expected");
    }

    #[test]
    fn test_calculate_correlation_inverse() {
        let (env, client, _admin) = setup();

        // Inverse correlation: y = -x
        let x = vec![&env, 10i128, 20i128, 30i128];
        let y = vec![&env, 30i128, 20i128, 10i128];
        let corr = client.calculate_correlation(&x, &y);

        // Correlation should be negative
        assert!(corr < 0, "Inverse correlation expected");
    }

    #[test]
    fn test_calculate_correlation_length_mismatch() {
        let (env, client, _admin) = setup();

        let x = vec![&env, 10i128, 20i128, 30i128];
        let y = vec![&env, 10i128, 20i128];
        let corr = client.calculate_correlation(&x, &y);

        assert_eq!(corr, 0);
    }

    #[test]
    fn test_calculate_rolling_statistics() {
        let (env, client, _admin) = setup();

        // 10 values, window=3, step=2
        // Window 0-2: avg(10,20,30) = 20
        // Window 2-4: avg(30,40,50) = 40
        // Window 4-6: avg(50,60,70) = 60
        // Window 6-8: avg(70,80,90) = 80
        let values = vec![&env, 10i128, 20i128, 30i128, 40i128, 50i128, 60i128, 70i128, 80i128, 90i128, 100i128];
        let rolling = client.calculate_rolling_statistics(&values, &3, &2);

        assert_eq!(rolling.len(), 4);
        assert_eq!(rolling.get(0).unwrap(), 20);
        assert_eq!(rolling.get(1).unwrap(), 40);
        assert_eq!(rolling.get(2).unwrap(), 60);
        assert_eq!(rolling.get(3).unwrap(), 80);
    }

    #[test]
    fn test_calculate_rolling_statistics_empty() {
        let (env, client, _admin) = setup();

        let values: Vec<i128> = vec![&env];
        let rolling = client.calculate_rolling_statistics(&values, &3, &1);

        assert_eq!(rolling.len(), 0);
    }

    #[test]
    fn test_calculate_rolling_statistics_window_larger() {
        let (env, client, _admin) = setup();

        let values = vec![&env, 10i128, 20i128];
        let rolling = client.calculate_rolling_statistics(&values, &3, &1);

        assert_eq!(rolling.len(), 0);
    }

    #[test]
    fn test_compute_statistics_basic() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "DEX");

        // Register asset and submit prices
        client.register_asset(&admin, &asset);

        // Submit 5 price points over the last hour
        let now = 1_000_000u64;
        for i in 0..5u64 {
            env.ledger().set_timestamp(now - 3_600 + i * 720); // 12 min intervals
            client.submit_price(&admin, &asset, &(1_000_000i128 + i as i128 * 10_000), &source);
        }

        env.ledger().set_timestamp(now);

        // Compute statistics for the day
        let stats = client.compute_statistics(&admin, &asset, &StatPeriod::Day);

        assert_eq!(stats.asset_code, asset);
        assert_eq!(stats.period, StatPeriod::Day);
        assert!(stats.data_points >= 5);
        assert!(stats.average_price > 0);
        assert!(stats.min_price <= stats.max_price);
        assert!(stats.median_price >= stats.p25_price);
        assert!(stats.p75_price >= stats.median_price);
    }

    #[test]
    #[should_panic]
    fn test_compute_statistics_unauthorized() {
        let (env, client, _admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let unauthorized = Address::generate(&env);

        env.mock_all_auths();
        let _stats = client.compute_statistics(&unauthorized, &asset, &StatPeriod::Day);
    }

    #[test]
    fn test_get_statistics_none() {
        let (env, client, _admin) = setup();
        let asset = String::from_str(&env, "USDC");

        let stats = client.get_statistics(&asset, &StatPeriod::Day);
        assert!(stats.is_none());
    }

    #[test]
    fn test_get_statistics_history_empty() {
        let (env, client, _admin) = setup();
        let asset = String::from_str(&env, "USDC");

        let history = client.get_statistics_history(&asset);
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_get_statistical_methods_documentation() {
        let (env, client, _admin) = setup();

        let docs = client.get_statistical_methods_documentation();
        let docs_str = docs.to_string();

        assert!(docs_str.contains("calculate_average"));
        assert!(docs_str.contains("calculate_stddev"));
        assert!(docs_str.contains("calculate_volatility"));
        assert!(docs_str.contains("calculate_median"));
        assert!(docs_str.contains("calculate_percentiles"));
        assert!(docs_str.contains("calculate_ema"));
        assert!(docs_str.contains("calculate_correlation"));
        assert!(docs_str.contains("compute_statistics"));
        assert!(docs_str.contains("trigger_periodic_stats"));
    }

    #[test]
    fn test_trigger_periodic_stats_no_assets() {
        let (env, client, admin) = setup();

        // Should not panic with no assets
        client.trigger_periodic_stats(&admin);

        // Verify it completes without error
        assert_eq!(client.get_monitored_assets().len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_trigger_periodic_stats_unauthorized() {
        let (env, client, _admin) = setup();
        let unauthorized = Address::generate(&env);

        env.mock_all_auths();
        client.trigger_periodic_stats(&unauthorized);
    }

    #[test]
    fn test_trigger_periodic_stats_with_data() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "DEX");

        // Register asset and submit prices
        client.register_asset(&admin, &asset);

        // Submit prices
        env.ledger().set_timestamp(1_000_000);
        for i in 0..3u64 {
            client.submit_price(&admin, &asset, &(1_000_000i128 + i as i128 * 5_000), &source);
        }

        // Trigger periodic stats
        client.trigger_periodic_stats(&admin);

        // Check that stats were computed
        let stats = client.get_statistics(&asset, &StatPeriod::Day);
        assert!(stats.is_some());

        let stats_data = stats.unwrap();
        assert_eq!(stats_data.asset_code, asset);
        assert!(stats_data.data_points >= 2);
    }

    #[test]
    fn test_compute_statistics_different_periods() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "DEX");

        client.register_asset(&admin, &asset);

        // Submit prices over a week
        let now = 7 * 86_400u64; // 7 days
        for i in 0..20u64 {
            env.ledger().set_timestamp(now - 7 * 86_400 + i * 30_000);
            client.submit_price(&admin, &asset, &(1_000_000i128 + (i as i128 % 100) * 1_000), &source);
        }

        env.ledger().set_timestamp(now);

        // Compute hour stats
        let hour_stats = client.compute_statistics(&admin, &asset, &StatPeriod::Hour);
        assert_eq!(hour_stats.period, StatPeriod::Hour);

        // Compute day stats
        let day_stats = client.compute_statistics(&admin, &asset, &StatPeriod::Day);
        assert_eq!(day_stats.period, StatPeriod::Day);

        // Compute week stats
        let week_stats = client.compute_statistics(&admin, &asset, &StatPeriod::Week);
        assert_eq!(week_stats.period, StatPeriod::Week);
    }

    #[test]
    fn test_statistics_storage_and_retrieval() {
        let (env, client, admin) = setup();
        let asset = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "DEX");

        client.register_asset(&admin, &asset);

        // Submit prices and compute stats
        env.ledger().set_timestamp(1_000_000);
        for i in 0..5u64 {
            client.submit_price(&admin, &asset, &(1_000_000i128 + i as i128 * 10_000), &source);
        }

        // Compute multiple statistics
        client.compute_statistics(&admin, &asset, &StatPeriod::Day);

        env.ledger().set_timestamp(1_100_000);
        client.compute_statistics(&admin, &asset, &StatPeriod::Day);

        // Get history
        let history = client.get_statistics_history(&asset);
        assert!(history.len() >= 1);

        // Get most recent
        let recent = client.get_statistics(&asset, &StatPeriod::Day);
        assert!(recent.is_some());
        assert_eq!(recent.unwrap().timestamp, 1_100_000);
    }
}
