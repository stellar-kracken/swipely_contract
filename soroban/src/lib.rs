#![no_std]

// governance and insurance_pool are standalone contracts — only compiled for

// tests (native target) to avoid Wasm symbol conflicts with BridgeWatchContract.
#[cfg(test)]

pub mod governance;
pub mod liquidity_pool;
pub mod reputation_system;
pub mod multisig_treasury;
#[cfg(test)]

pub mod insurance_pool;
#[cfg(test)]
pub mod reputation_system;
pub mod source_trust;

pub mod rate_limiter;
pub mod migration;

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
// Storage key constants instead of using DataKey enum for storage operations
mod keys {
    pub const ADMIN: &str = "admin";
    pub const ASSET_HEALTH: &str = "asset_health";
    pub const PRICE_RECORD: &str = "price_record";
    pub const MONITORED_ASSETS: &str = "monitored_assets";
    pub const DEVIATION_ALERT: &str = "deviation_alert";
    pub const DEVIATION_THRESHOLD: &str = "deviation_threshold";
    pub const SUPPLY_MISMATCHES: &str = "supply_mismatches";
    pub const MISMATCH_THRESHOLD: &str = "mismatch_threshold";
    pub const BRIDGE_IDS: &str = "bridge_ids";
    pub const ROLE_KEY: &str = "role_key";
    pub const ROLES_LIST: &str = "roles_list";
    pub const SIGNER: &str = "signer";
    pub const SIGNER_LIST: &str = "signer_list";
    pub const SIGNATURE_THRESHOLD: &str = "signature_threshold";
    pub const SIGNER_NONCE: &str = "signer_nonce";
    pub const SIGNATURE_CACHE: &str = "signature_cache";
    pub const LIQUIDITY_DEPTH: &str = "liquidity_depth";
    pub const LIQUIDITY_HISTORY: &str = "liquidity_history";
    pub const LIQUIDITY_PAIRS: &str = "liquidity_pairs";
    pub const PRICE_HISTORY: &str = "price_history";
    pub const HEALTH_WEIGHTS: &str = "health_weights";
    pub const HEALTH_SCORE_RESULT: &str = "health_score_result";
    pub const RISK_SCORE_CONFIG: &str = "risk_score_config";
    pub const CHECKPOINT_CONFIG: &str = "checkpoint_config";
    pub const CHECKPOINT_COUNTER: &str = "checkpoint_counter";
    pub const CHECKPOINT_METADATA_LIST: &str = "checkpoint_metadata_list";
    pub const CHECKPOINT_SNAPSHOT: &str = "checkpoint_snapshot";
    pub const LAST_CHECKPOINT_AT: &str = "last_checkpoint_at";
    pub const LAST_CHECKPOINT_ID: &str = "last_checkpoint_id";
    pub const RETENTION_POLICY: &str = "retention_policy";
    pub const ASSET_RETENTION_OVR: &str = "asset_retention_ovr";
    pub const LAST_CLEANUP_AT: &str = "last_cleanup_at";
    pub const ARCHIVED_MISMATCHES: &str = "archived_mismatches";
    pub const ARCHIVED_LIQUIDITY_HISTORY: &str = "archived_liquidity_history";
    pub const ARCHIVED_CHECKPOINT_META: &str = "archived_checkpoint_meta";
    pub const ARCHIVED_CHECKPOINT_SNAPSHOT: &str = "archived_checkpoint_snapshot";
    pub const GLOBAL_PAUSED: &str = "global_paused";
    pub const PAUSE_GUARDIAN: &str = "pause_guardian";
    pub const PAUSE_REASON: &str = "pause_reason";
    pub const PAUSED_AT: &str = "paused_at";
    pub const UNPAUSE_AVAILABLE_AT: &str = "unpause_available_at";
    pub const PAUSE_HISTORY: &str = "pause_history";
    pub const EMERGENCY_CONTACT: &str = "emergency_contact";
    pub const ASSET_PAUSE_REASON: &str = "asset_pause_reason";
    pub const PENDING_TRANSFER: &str = "pending_transfer";
    pub const PENDING_UPGRADE: &str = "pending_upgrade";
    pub const UPGRADE_PROPOSAL_COUNTER: &str = "upgrade_proposal_counter";
    pub const UPGRADE_HISTORY: &str = "upgrade_history";
    pub const CONTRACT_VERSION: &str = "contract_version";
    pub const CURRENT_CONTRACT_WASM_HASH: &str = "current_wasm_hash";
    pub const ROLLBACK_TARGET_HASH: &str = "rollback_target_hash";
    pub const CONFIG_ENTRY: &str = "config_entry";
    pub const CONFIG_KEYS: &str = "config_keys";
    pub const CONFIG_AUDIT_LOG: &str = "config_audit_log";
    pub const ASSET_STATISTICS: &str = "asset_statistics";
    pub const EXPIRATIONPOLICY: &str = "expiration_policy";
    pub const CLEANUPSTATS: &str = "cleanup_stats";
    // Emergency Recovery (issue #298)
    pub const RECOVERY_MODE: &str = "recovery_mode";
    pub const RECOVERY_STEPS: &str = "recovery_steps";
    pub const RECOVERY_REASON: &str = "recovery_reason";
    // Trusted Source Registry
    pub const TRUSTED_SOURCE: &str = "trusted_source";
    pub const ALL_TRUSTED_SOURCES: &str = "all_trusted_sources";
    pub const RECOVERY_ENTERED_AT: &str = "recovery_entered_at";
    pub const RECOVERY_ENTERED_BY: &str = "recovery_entered_by";
    // Admin Activity Service (issue #299)
    pub const ADMIN_ACTIVITY_LOG: &str = "admin_activity_log";
    pub const ADMIN_ACTIVITY_CTR: &str = "admin_activity_ctr";
    // Multi-Source Health Submission (issue #300)
    pub const HEALTH_SOURCES: &str = "health_sources";
    // Event Replay Helpers (issue #296)
    pub const EVENT_REPLAY_LOG: &str = "event_replay_log";
    pub const EVENT_REPLAY_CTR: &str = "event_replay_ctr";
    // Contract State Migration (issue #403)
    pub const MIGRATION_VERSION: &str = "mig_version";
    pub const MIGRATION_HISTORY: &str = "mig_history";
}

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



/// Configurable weights for health score calculation.

///

/// Each weight is expressed as a percentage (0–100). The three weights must

/// sum to exactly 100. Default weights are: liquidity 30 %, price stability

/// 40 %, bridge uptime 30 %.

#[contracttype]

#[derive(Clone, Debug, Eq, PartialEq)]

pub struct HealthWeights {

    /// Weight assigned to the liquidity component (default 30).

    pub liquidity_weight: u32,

    /// Weight assigned to the price stability component (default 40).

    pub price_stability_weight: u32,

    /// Weight assigned to the bridge uptime component (default 30).

    pub bridge_uptime_weight: u32,

    /// Methodology version identifier for auditability.

    pub version: u32,

}



/// Result of an automated health score calculation.

///

/// Returned by `calculate_health_score()` and stored alongside the

/// `AssetHealth` record when using `submit_calculated_health()`.

#[contracttype]

#[derive(Clone, Debug, Eq, PartialEq)]

pub struct HealthScoreResult {

    /// Composite health score (0–100).

    pub composite_score: u32,

    /// Liquidity component score that was used (0–100).

    pub liquidity_score: u32,

    /// Price stability component score that was used (0–100).

    pub price_stability_score: u32,

    /// Bridge uptime component score that was used (0–100).

    pub bridge_uptime_score: u32,

    /// Weights that were applied during calculation.

    pub weights: HealthWeights,

    /// Ledger timestamp when the calculation was performed.

    pub timestamp: u64,
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

#[derive(Clone, Debug, Eq, PartialEq)]

pub enum StatusTier {

    Ok,

    Low,

    Medium,

    High,

}


#[contracttype]

#[derive(Clone, Debug, Eq, PartialEq)]

pub struct AssetStatusRollup {

    pub asset_code: String,

    pub tier: StatusTier,

    pub health_score: u32,

    pub has_price_deviation_alert: bool,

    pub price_deviation_tier: StatusTier,

    pub paused: bool,

    pub active: bool,

    pub timestamp: u64,

}


#[contracttype]

#[derive(Clone, Debug, Eq, PartialEq)]

pub struct BridgeStatusRollup {

    pub bridge_id: String,

    pub tier: StatusTier,

    pub latest_mismatch_bps: i128,

    pub is_critical: bool,

    pub timestamp: u64,

}


#[contracttype]

#[derive(Clone, Debug, Eq, PartialEq)]

pub struct ContractStatusRollup {

    pub tier: StatusTier,

    pub asset_ok: u32,

    pub asset_low: u32,

    pub asset_medium: u32,

    pub asset_high: u32,

    pub bridge_ok: u32,

    pub bridge_low: u32,

    pub bridge_medium: u32,

    pub bridge_high: u32,

    pub timestamp: u64,

}


// ---------------------------------------------------------------------------
// Emergency Pause types (issue #96)
// ---------------------------------------------------------------------------

/// A single entry in the contract's pause/unpause audit log.
///
/// Emitted and stored whenever the global pause state changes so that
/// operators can trace the full history of emergency actions.
#[contracttype]
pub enum DataKey {

    Admin,
    HealthWeights,
    HealthScoreResult(String),
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

    /// Latest rollup status for an asset.
    AssetStatusRollup(String),

    /// Latest rollup status for a bridge.
    BridgeStatusRollup(String),

    /// Latest rollup status for the whole contract.
    ContractStatusRollup,
}

#[contract]

pub struct BridgeWatchContract;



#[allow(clippy::too_many_arguments)]
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


    pub fn get_asset_status_rollup(env: Env, asset_code: String) -> Option<AssetStatusRollup> {

        env.storage()

            .persistent()

            .get(&DataKey::AssetStatusRollup(asset_code))

    }


    pub fn get_bridge_status_rollup(env: Env, bridge_id: String) -> Option<BridgeStatusRollup> {

        env.storage()

            .persistent()

            .get(&DataKey::BridgeStatusRollup(bridge_id))

    }


    pub fn get_contract_status_rollup(env: Env) -> ContractStatusRollup {

        env.storage()

            .persistent()

            .get(&DataKey::ContractStatusRollup)

            .unwrap_or(ContractStatusRollup {

                tier: StatusTier::Ok,

                asset_ok: 0,

                asset_low: 0,

                asset_medium: 0,

                asset_high: 0,

                bridge_ok: 0,

                bridge_low: 0,

                bridge_medium: 0,

                bridge_high: 0,

                timestamp: env.ledger().timestamp(),

            })

    }



    /// Submit a health score for a monitored asset.

    ///

    /// `caller` must be the contract admin, a `SuperAdmin`, or a

    /// `HealthSubmitter`. Backward compatible: the original admin address

    /// requires no explicit role assignment.
    ///
    /// Additionally, if source trust is enabled, the caller must be a
    /// registered trusted source.
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
        
        // Check if caller is a trusted source (if any sources are registered)
        let active_sources = source_trust::get_active_trusted_sources(&env);
        if active_sources.len() > 0 {
            // If sources are registered, enforce trust requirement
            source_trust::require_trusted_source(&env, &caller);
        }
        
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
            .set(&DataKey::AssetHealth(asset_code.clone()), &record);


        Self::update_asset_rollup(&env, &asset_code);



        env.events()
            .publish((symbol_short!("health_up"), asset_code), health_score);
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


            Self::update_asset_rollup(&env, &item.asset_code);



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
    ///
    /// Additionally, if source trust is enabled, the caller must be a
    /// registered trusted source.
    pub fn submit_price(

        env: Env,

        caller: Address,

        asset_code: String,

        price: i128,

        source: String,

    ) {
        Self::check_permission(&env, &caller, AdminRole::PriceSubmitter);
        
        // Check if caller is a trusted source (if any sources are registered)
        let active_sources = source_trust::get_active_trusted_sources(&env);
        if active_sources.len() > 0 {
            // If sources are registered, enforce trust requirement
            source_trust::require_trusted_source(&env, &caller);
        }
        
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



        env.events()

            .publish((symbol_short!("price_up"), asset_code), price);
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



        let timestamp = env.ledger().timestamp();
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


        Self::update_asset_rollup(&env, &asset_code);



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
            .set(&DataKey::DeviationThreshold(asset_code.clone()), &threshold);



        env.events()

            .publish((symbol_short!("thresh_up"), asset_code), low_bps);
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
            .set(&DataKey::DeviationAlert(asset_code.clone()), &alert);


        Self::update_asset_rollup(&env, &asset_code);



        env.events()

            .publish((symbol_short!("price_dev"), asset_code), deviation_bps);



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

        env.events()
            .publish((symbol_short!("thresh_up"), symbol_short!("mismatch")), threshold_bps);
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


        Self::update_bridge_rollup(&env, &bridge_id, mismatch_bps, is_critical);



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

            bridge_ids.push_back(bridge_id.clone());
            env.storage()
                .instance()
                .set(&DataKey::BridgeIds, &bridge_ids);
        }



        env.events()

            .publish((symbol_short!("supply_mm"), bridge_id), mismatch_bps);
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

            pairs.push_back(asset_pair.clone());
            env.storage()
                .instance()
                .set(&DataKey::LiquidityPairs, &pairs);
        }



        env.events()

            .publish((symbol_short!("liq_chg"), asset_pair), total_liquidity);
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

            address: grantee.clone(),

            role: role.clone(),

        });

        env.storage()

            .persistent()
            .set(&DataKey::RolesList, &assignments);



        env.events()
            .publish((symbol_short!("role_grnt"), grantee), role);
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



        env.events()
            .publish((symbol_short!("role_revk"), target), role);
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



    #[allow(clippy::too_many_arguments)]
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


    fn tier_rank(tier: &StatusTier) -> u32 {

        match tier {

            StatusTier::Ok => 0,

            StatusTier::Low => 1,

            StatusTier::Medium => 2,

            StatusTier::High => 3,

        }

    }


    fn max_tier(a: StatusTier, b: StatusTier) -> StatusTier {

        if Self::tier_rank(&a) >= Self::tier_rank(&b) {

            a

        } else {

            b

        }

    }


    fn health_to_tier(score: u32) -> StatusTier {

        if score >= 80 {

            StatusTier::Ok

        } else if score >= 60 {

            StatusTier::Low

        } else if score >= 40 {

            StatusTier::Medium

        } else {

            StatusTier::High

        }

    }


    fn deviation_to_tier(alert: &Option<DeviationAlert>) -> (bool, StatusTier) {

        match alert {

            None => (false, StatusTier::Ok),

            Some(a) => match a.severity {

                DeviationSeverity::Low => (true, StatusTier::Low),

                DeviationSeverity::Medium => (true, StatusTier::Medium),

                DeviationSeverity::High => (true, StatusTier::High),

            },

        }

    }


    fn compute_contract_tier_from_counts(rollup: &ContractStatusRollup) -> StatusTier {

        if rollup.asset_high > 0 || rollup.bridge_high > 0 {

            StatusTier::High

        } else if rollup.asset_medium > 0 || rollup.bridge_medium > 0 {

            StatusTier::Medium

        } else if rollup.asset_low > 0 || rollup.bridge_low > 0 {

            StatusTier::Low

        } else {

            StatusTier::Ok

        }

    }


    fn bump_contract_counts_for_asset(env: &Env, prev: Option<StatusTier>, next: StatusTier) {

        let mut rollup: ContractStatusRollup = env

            .storage()

            .persistent()

            .get(&DataKey::ContractStatusRollup)

            .unwrap_or(ContractStatusRollup {

                tier: StatusTier::Ok,

                asset_ok: 0,

                asset_low: 0,

                asset_medium: 0,

                asset_high: 0,

                bridge_ok: 0,

                bridge_low: 0,

                bridge_medium: 0,

                bridge_high: 0,

                timestamp: env.ledger().timestamp(),

            });


        if let Some(p) = prev {

            match p {

                StatusTier::Ok => rollup.asset_ok = rollup.asset_ok.saturating_sub(1),

                StatusTier::Low => rollup.asset_low = rollup.asset_low.saturating_sub(1),

                StatusTier::Medium => rollup.asset_medium = rollup.asset_medium.saturating_sub(1),

                StatusTier::High => rollup.asset_high = rollup.asset_high.saturating_sub(1),

            }

        }


        match next {

            StatusTier::Ok => rollup.asset_ok += 1,

            StatusTier::Low => rollup.asset_low += 1,

            StatusTier::Medium => rollup.asset_medium += 1,

            StatusTier::High => rollup.asset_high += 1,

        }


        rollup.tier = Self::compute_contract_tier_from_counts(&rollup);

        rollup.timestamp = env.ledger().timestamp();


        env.storage()

            .persistent()

            .set(&DataKey::ContractStatusRollup, &rollup);


        env.events().publish((symbol_short!("ctr_st"),), rollup.tier.clone());

    }


    fn bump_contract_counts_for_bridge(env: &Env, prev: Option<StatusTier>, next: StatusTier) {

        let mut rollup: ContractStatusRollup = env

            .storage()

            .persistent()

            .get(&DataKey::ContractStatusRollup)

            .unwrap_or(ContractStatusRollup {

                tier: StatusTier::Ok,

                asset_ok: 0,

                asset_low: 0,

                asset_medium: 0,

                asset_high: 0,

                bridge_ok: 0,

                bridge_low: 0,

                bridge_medium: 0,

                bridge_high: 0,

                timestamp: env.ledger().timestamp(),

            });


        if let Some(p) = prev {

            match p {

                StatusTier::Ok => rollup.bridge_ok = rollup.bridge_ok.saturating_sub(1),

                StatusTier::Low => rollup.bridge_low = rollup.bridge_low.saturating_sub(1),

                StatusTier::Medium => rollup.bridge_medium = rollup.bridge_medium.saturating_sub(1),

                StatusTier::High => rollup.bridge_high = rollup.bridge_high.saturating_sub(1),

            }

        }


        match next {

            StatusTier::Ok => rollup.bridge_ok += 1,

            StatusTier::Low => rollup.bridge_low += 1,

            StatusTier::Medium => rollup.bridge_medium += 1,

            StatusTier::High => rollup.bridge_high += 1,

        }


        rollup.tier = Self::compute_contract_tier_from_counts(&rollup);

        rollup.timestamp = env.ledger().timestamp();


        env.storage()

            .persistent()

            .set(&DataKey::ContractStatusRollup, &rollup);


        env.events().publish((symbol_short!("ctr_st"),), rollup.tier.clone());

    }


    fn update_asset_rollup(env: &Env, asset_code: &String) {

        let health = Self::load_asset_health(env, asset_code);

        let deviation: Option<DeviationAlert> = env

            .storage()

            .persistent()

            .get(&DataKey::DeviationAlert(asset_code.clone()));


        let health_tier = Self::health_to_tier(health.health_score);

        let (has_alert, deviation_tier) = Self::deviation_to_tier(&deviation);

        let mut tier = Self::max_tier(health_tier, deviation_tier.clone());


        if !health.active {

            tier = Self::max_tier(tier, StatusTier::Low);

        }

        if health.paused {

            tier = Self::max_tier(tier, StatusTier::Low);

        }


        let prev: Option<AssetStatusRollup> = env

            .storage()

            .persistent()

            .get(&DataKey::AssetStatusRollup(asset_code.clone()));

        let prev_tier = prev.as_ref().map(|r| r.tier.clone());


        let rollup = AssetStatusRollup {

            asset_code: asset_code.clone(),

            tier: tier.clone(),

            health_score: health.health_score,

            has_price_deviation_alert: has_alert,

            price_deviation_tier: deviation_tier,

            paused: health.paused,

            active: health.active,

            timestamp: env.ledger().timestamp(),

        };


        env.storage()

            .persistent()

            .set(&DataKey::AssetStatusRollup(asset_code.clone()), &rollup);


        Self::bump_contract_counts_for_asset(env, prev_tier, tier.clone());

        env.events().publish((symbol_short!("asset_st"), asset_code.clone()), tier);

    }


    fn update_bridge_rollup(env: &Env, bridge_id: &String, mismatch_bps: i128, is_critical: bool) {

        let tier = if is_critical {

            StatusTier::High

        } else {

            StatusTier::Ok

        };


        let prev: Option<BridgeStatusRollup> = env

            .storage()

            .persistent()

            .get(&DataKey::BridgeStatusRollup(bridge_id.clone()));

        let prev_tier = prev.as_ref().map(|r| r.tier.clone());


        let rollup = BridgeStatusRollup {

            bridge_id: bridge_id.clone(),

            tier: tier.clone(),

            latest_mismatch_bps: mismatch_bps,

            is_critical,

            timestamp: env.ledger().timestamp(),

        };


        env.storage()

            .persistent()

            .set(&DataKey::BridgeStatusRollup(bridge_id.clone()), &rollup);


        Self::bump_contract_counts_for_bridge(env, prev_tier, tier.clone());

        env.events().publish((symbol_short!("bridge_st"), bridge_id.clone()), tier);

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

    // Automated health score calculation (issue #26)

    // -----------------------------------------------------------------------



    /// Set configurable weights used by the automated health score calculation.

    ///

    /// `caller` must be the contract admin or a `SuperAdmin`. The three weights

    /// must each be in the range 0–100 and must sum to exactly 100. The

    /// `version` field tracks the methodology revision for auditability.

    ///

    /// # Panics

    /// - Caller is not authorised.

    /// - Any individual weight exceeds 100.

    /// - The weights do not sum to 100.

    /// - `version` is 0.

    pub fn set_health_weights(

        env: Env,

        caller: Address,

        liquidity_weight: u32,

        price_stability_weight: u32,

        bridge_uptime_weight: u32,

        version: u32,

    ) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let authorized =

            caller == admin || Self::has_role_internal(&env, &caller, AdminRole::SuperAdmin);

        if !authorized {

            panic!("only admin or SuperAdmin can set health weights");

        }



        Self::validate_weights(liquidity_weight, price_stability_weight, bridge_uptime_weight);
        if version == 0 {

            panic!("methodology version must be greater than 0");

        }



        let weights = HealthWeights {

            liquidity_weight,

            price_stability_weight,

            bridge_uptime_weight,

            version,

        };



        env.storage()

            .instance()
            .set(&DataKey::HealthWeights, &weights);

        env.events()
            .publish((symbol_short!("wt_set"),), version);
    }



    /// Return the current health score calculation weights.

    ///

    /// Public read access — no authorisation required. Returns the

    /// admin-configured weights or the defaults (30 / 40 / 30, version 1)

    /// when none have been explicitly set.

    pub fn get_health_weights(env: Env) -> HealthWeights {

        Self::load_health_weights(&env)

    }



    /// Pure calculation: compute a composite health score from component

    /// scores using the stored (or default) weights.

    ///

    /// This function does **not** store any result on-chain; it is intended

    /// for off-chain callers that want to preview the score before submitting.

    ///

    /// Formula:

    /// ```text

    /// composite = (liquidity * liq_w + stability * stab_w + uptime * up_w) / 100

    /// ```

    ///

    /// All input scores must be in the 0–100 range.

    ///

    /// # Panics

    /// - Any input score is greater than 100.

    pub fn calculate_health_score(

        env: Env,

        liquidity_score: u32,

        price_stability_score: u32,

        bridge_uptime_score: u32,

    ) -> HealthScoreResult {

        Self::validate_score_range(liquidity_score, "liquidity_score");

        Self::validate_score_range(price_stability_score, "price_stability_score");

        Self::validate_score_range(bridge_uptime_score, "bridge_uptime_score");



        let weights = Self::load_health_weights(&env);

        let composite = Self::compute_composite(

            liquidity_score,

            price_stability_score,

            bridge_uptime_score,

            &weights,

        );



        HealthScoreResult {

            composite_score: composite,

            liquidity_score,

            price_stability_score,

            bridge_uptime_score,

            weights,

            timestamp: env.ledger().timestamp(),
        }

    }



    /// Submit a health score that is **automatically calculated** from the

    /// supplied component scores using the stored weights.

    ///

    /// This is the recommended entry-point for Phase 1 MVP health scoring. It

    /// combines `calculate_health_score()` with `submit_health()`, storing

    /// both the `AssetHealth` record and the detailed `HealthScoreResult`.

    ///

    /// `caller` must be the contract admin, a `SuperAdmin`, or a

    /// `HealthSubmitter`. The asset must be registered, active, and not paused.

    /// All component scores must be in the 0–100 range.

    ///

    /// An optional `manual_override` score (0–100) can replace the calculated

    /// composite score while still recording the underlying calculation for

    /// transparency.

    ///

    /// # Panics

    /// - Caller is not authorised.

    /// - Asset is not registered, deregistered, or paused.

    /// - Any component score is greater than 100.

    /// - `manual_override` is provided and exceeds 100.

    pub fn submit_calculated_health(

        env: Env,

        caller: Address,

        asset_code: String,

        liquidity_score: u32,

        price_stability_score: u32,

        bridge_uptime_score: u32,

        manual_override: Option<u32>,

    ) {
        Self::check_permission(&env, &caller, AdminRole::HealthSubmitter);

        let status = Self::load_asset_health(&env, &asset_code);

        Self::assert_asset_accepting_submissions(&status);



        Self::validate_score_range(liquidity_score, "liquidity_score");

        Self::validate_score_range(price_stability_score, "price_stability_score");

        Self::validate_score_range(bridge_uptime_score, "bridge_uptime_score");



        let weights = Self::load_health_weights(&env);

        let calculated_composite = Self::compute_composite(

            liquidity_score,

            price_stability_score,

            bridge_uptime_score,

            &weights,

        );



        let final_score = match manual_override {

            Some(override_score) => {

                Self::validate_score_range(override_score, "manual_override");

                override_score

            }

            None => calculated_composite,

        };



        let timestamp = env.ledger().timestamp();



        let record = AssetHealth {

            asset_code: asset_code.clone(),

            health_score: final_score,

            liquidity_score,

            price_stability_score,

            bridge_uptime_score,

            paused: status.paused,

            active: status.active,

            timestamp,
        };



        let result = HealthScoreResult {

            composite_score: calculated_composite,

            liquidity_score,

            price_stability_score,

            bridge_uptime_score,

            weights,

            timestamp,
        };



        env.storage()

            .persistent()
            .set(&DataKey::AssetHealth(asset_code.clone()), &record);
        env.storage()

            .persistent()
            .set(&DataKey::HealthScoreResult(asset_code.clone()), &result);

        env.events().publish(
            (symbol_short!("health_up"), asset_code),
            final_score,
        );
    }



    /// Return the latest calculated health score result for an asset.

    ///

    /// Public read access — no authorisation required. Returns `None` if no

    /// calculated score has been submitted for the asset.

    pub fn get_health_score_result(env: Env, asset_code: String) -> Option<HealthScoreResult> {

        env.storage()

            .persistent()
            .get(&DataKey::HealthScoreResult(asset_code))
    }



    // -----------------------------------------------------------------------

    // Private helpers — health score calculation

    // -----------------------------------------------------------------------

    /// Load stored health weights or return defaults (30 / 40 / 30, v1).

    fn load_health_weights(env: &Env) -> HealthWeights {

        env.storage()

            .instance()
            .get(&DataKey::HealthWeights)
            .unwrap_or(HealthWeights {

                liquidity_weight: 30,

                price_stability_weight: 40,

                bridge_uptime_weight: 30,

                version: 1,

            })

    }

    /// Validate that three weights are each ≤ 100 and sum to exactly 100.

    fn validate_weights(liq: u32, stab: u32, up: u32) {

        if liq > 100 || stab > 100 || up > 100 {

            panic!("each weight must be between 0 and 100");

        }

        if liq + stab + up != 100 {

            panic!("weights must sum to 100");

        }

    }



    /// Validate that a single score is within the 0–100 range.

    fn validate_score_range(score: u32, name: &str) {

        if score > 100 {

            panic!("{} must be between 0 and 100", name);

        }

    }

    /// Compute the weighted-average composite score.

    ///

    /// `composite = (liq * liq_w + stab * stab_w + up * up_w) / 100`

    fn compute_composite(

        liquidity_score: u32,

        price_stability_score: u32,

        bridge_uptime_score: u32,

        weights: &HealthWeights,

    ) -> u32 {

        let weighted_sum = (liquidity_score as u64) * (weights.liquidity_weight as u64)

            + (price_stability_score as u64) * (weights.price_stability_weight as u64)

            + (bridge_uptime_score as u64) * (weights.bridge_uptime_weight as u64);

        (weighted_sum / 100) as u32

    }
}



#[cfg(test)]

    fn emit_contract_event(env: &Env, event: BridgeWatchEvent) {
        match event {
            BridgeWatchEvent::HealthSubmitted {
                actor,
                asset_code,
                health_score,
                timestamp,
            } => {
                env.events().publish(
                    (symbol_short!("hlth_sub"), actor, asset_code),
                    (health_score, timestamp),
                );
            }
            BridgeWatchEvent::ThresholdUpdated {
                actor,
                scope,
                value,
                timestamp,
            } => {
                env.events()
                    .publish((symbol_short!("thr_upd"), actor, scope), (value, timestamp));
            }
            BridgeWatchEvent::RoleChanged {
                actor,
                target,
                granted,
                role,
                timestamp,
            } => {
                env.events().publish(
                    (symbol_short!("role_chg"), actor, target),
                    (granted, role, timestamp),
                );
            }
            BridgeWatchEvent::ExpirationPolicyUpdated {
                actor,
                scope,
                ttl_secs,
                timestamp,
            } => {
                env.events().publish(
                    (symbol_short!("exp_upd"), actor, scope),
                    (ttl_secs, timestamp),
                );
            }
            BridgeWatchEvent::ExpirationExtended {
                actor,
                scope,
                expires_at,
                timestamp,
            } => {
                env.events().publish(
                    (symbol_short!("exp_ext"), actor, scope),
                    (expires_at, timestamp),
                );
            }
            BridgeWatchEvent::CleanupCompleted {
                actor,
                removed_records,
                trimmed_history_records,
                timestamp,
            } => {
                env.events().publish(
                    (symbol_short!("cleanup"), actor),
                    (removed_records, trimmed_history_records, timestamp),
                );
            }
            _ => {}
        }
    }

    fn maybe_trigger_auto_cleanup(env: &Env) {
        let now = env.ledger().timestamp();
        let mut total_deleted = 0u32;
        let mut total_archived = 0u32;

        for data_type in Self::retention_data_types(env).iter() {
            let policy = Self::load_retention_policy(env, &data_type);
            if !policy.enabled {
                continue;
            }

            let last_cleanup_at: u64 = env
                .storage()
                .instance()
                .get(&ConfigDataKey::LastCleanup(data_type.clone()))
                .unwrap_or(0);
            if last_cleanup_at != 0 && now < last_cleanup_at + policy.trigger_interval_secs {
                continue;
            }

            let (deleted, archived) = Self::cleanup_data_type_internal(
                env,
                &data_type,
                &policy,
                policy.max_deletions_per_run,
            );
            env.storage()
                .instance()
                .set(&ConfigDataKey::LastCleanup(data_type.clone()), &now);

            if deleted > 0 || archived > 0 {
                env.events().publish(
                    (
                        symbol_short!("ret_auto"),
                        Self::retention_kind_code(&data_type),
                    ),
                    (deleted, archived, now),
                );
            }

            total_deleted += deleted;
            total_archived += archived;
        }

        if total_deleted > 0 || total_archived > 0 {
            env.events().publish(
                (symbol_short!("ret_job"),),
                (total_deleted, total_archived, now),
            );
        }
    }

    fn supply_storage_usage(env: &Env) -> StorageUsageEntry {
        let bridge_ids: Vec<String> = env
            .storage()
            .instance()
            .get(&keys::BRIDGE_IDS)
            .unwrap_or_else(|| Vec::new(env));

        let mut active_records = 0u32;
        let mut archived_records = 0u32;
        for bridge_id in bridge_ids.iter() {
            let active: Vec<SupplyMismatch> = env
                .storage()
                .persistent()
                .get(&BridgeDataKey::Mismatches(bridge_id.clone()))
                .unwrap_or_else(|| Vec::new(env));
            let archived: Vec<SupplyMismatch> = env
                .storage()
                .persistent()
                .get(&BridgeDataKey::ArchMismatches(bridge_id))
                .unwrap_or_else(|| Vec::new(env));
            active_records += active.len();
            archived_records += archived.len();
        }

        StorageUsageEntry {
            data_type: RetentionDataType::SupplyMismatches,
            tracked_keys: bridge_ids.len(),
            active_records,
            archived_records,
        }
    }

    fn liquidity_storage_usage(env: &Env) -> StorageUsageEntry {
        let pairs: Vec<String> = env
            .storage()
            .instance()
            .get(&keys::LIQUIDITY_PAIRS)
            .unwrap_or_else(|| Vec::new(env));

        let mut active_records = 0u32;
        let mut archived_records = 0u32;
        for pair in pairs.iter() {
            let active: Vec<LiquidityDepth> = env
                .storage()
                .persistent()
                .get(&AssetDataKey::LiqHist(pair.clone()))
                .unwrap_or_else(|| Vec::new(env));
            let archived: Vec<LiquidityDepth> = env
                .storage()
                .persistent()
                .get(&AssetDataKey::ArchLiqHist(pair))
                .unwrap_or_else(|| Vec::new(env));
            active_records += active.len();
            archived_records += archived.len();
        }

        StorageUsageEntry {
            data_type: RetentionDataType::LiquidityHistory,
            tracked_keys: pairs.len(),
            active_records,
            archived_records,
        }
    }

    fn checkpoint_storage_usage(env: &Env) -> StorageUsageEntry {
        let active_metadata = Self::load_checkpoint_metadata(env);
        let archived_metadata: Vec<CheckpointMetadata> = env
            .storage()
            .instance()
            .get(&keys::ARCHIVED_CHECKPOINT_META)
            .unwrap_or_else(|| Vec::new(env));

        StorageUsageEntry {
            data_type: RetentionDataType::Checkpoints,
            tracked_keys: active_metadata.len(),
            active_records: active_metadata.len(),
            archived_records: archived_metadata.len(),
        }
    }

    fn maybe_create_auto_checkpoint(env: &Env, caller: &Address) {
        let config = Self::load_checkpoint_config(env);
        let now = env.ledger().timestamp();
        let last_at: u64 = env
            .storage()
            .instance()
            .get(&keys::LAST_CHECKPOINT_AT)
            .unwrap_or(0);

        if last_at != 0 && now < last_at + config.interval_secs {
            return;
        }

        Self::persist_checkpoint(
            env,
            caller,
            CheckpointTrigger::Automatic,
            String::from_str(env, "auto"),
            None,
        );
    }

    fn persist_checkpoint(
        env: &Env,
        caller: &Address,
        trigger: CheckpointTrigger,
        label: String,
        restored_from: Option<u64>,
    ) -> CheckpointMetadata {
        let config = Self::load_checkpoint_config(env);
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&keys::CHECKPOINT_COUNTER)
            .unwrap_or(0)
            + 1;
        let created_at = env.ledger().timestamp();
        let monitored_assets = Self::load_registered_assets_raw(env);
        let health_weights = Self::load_health_weights(env);
        let risk_score_config = Self::load_risk_score_config(env);
        let mut assets = Vec::new(env);

        for asset_code in monitored_assets.iter() {
            let health = Self::load_asset_health(env, &asset_code);
            let latest_price_opt: Option<PriceRecord> = env
                .storage()
                .persistent()
                .get(&AssetDataKey::Price(asset_code.clone()));
            let health_result_opt: Option<HealthScoreResult> = env
                .storage()
                .persistent()
                .get(&AssetDataKey::HealthRes(asset_code.clone()));

            let default_price = PriceRecord {
                asset_code: asset_code.clone(),
                price: 0,
                source: String::from_str(env, ""),
                timestamp: 0,
                expires_at: 0,
            };
            let default_result = HealthScoreResult {
                composite_score: 0,
                liquidity_score: 0,
                price_stability_score: 0,
                bridge_uptime_score: 0,
                weights: Self::default_health_weights(),
                timestamp: 0,
                expires_at: 0,
            };

            assets.push_back(CheckpointAssetState {
                asset_code,
                health,
                has_latest_price: latest_price_opt.is_some(),
                latest_price: latest_price_opt.unwrap_or(default_price),
                has_health_result: health_result_opt.is_some(),
                health_result: health_result_opt.unwrap_or(default_result),
            });
        }

        let snapshot = CheckpointSnapshot {
            checkpoint_id: next_id,
            format_version: config.format_version,
            created_at,
            trigger: trigger.clone(),
            created_by: caller.clone(),
            label: label.clone(),
            monitored_assets: monitored_assets.clone(),
            health_weights,
            risk_score_config,
            assets,
            restored_from,
        };
        let state_hash = Self::compute_checkpoint_hash(env, &snapshot);
        let metadata = CheckpointMetadata {
            checkpoint_id: next_id,
            format_version: snapshot.format_version,
            created_at,
            trigger,
            created_by: caller.clone(),
            label,
            monitored_asset_count: snapshot.monitored_assets.len(),
            asset_count: snapshot.assets.len(),
            state_hash,
            restored_from,
        };

        env.storage()
            .persistent()
            .set(&ConfigDataKey::ChkpntSnap(next_id), &snapshot);

        let mut metadata_list = Self::load_checkpoint_metadata(env);
        metadata_list.push_back(metadata.clone());
        env.storage()
            .instance()
            .set(&keys::CHECKPOINT_METADATA_LIST, &metadata_list);
        env.storage()
            .instance()
            .set(&keys::CHECKPOINT_COUNTER, &next_id);
        env.storage()
            .instance()
            .set(&keys::LAST_CHECKPOINT_AT, &created_at);
        env.storage()
            .instance()
            .set(&keys::LAST_CHECKPOINT_ID, &next_id);

        Self::prune_checkpoints(env, &config);
        env.events()
            .publish((symbol_short!("chkptnew"), next_id), metadata.asset_count);
        Self::maybe_trigger_auto_cleanup(env);
        metadata
    }

    fn prune_checkpoints(env: &Env, config: &CheckpointConfig) {
        let mut metadata_list = Self::load_checkpoint_metadata(env);
        let mut pruned = 0u32;

        while metadata_list.len() > config.max_checkpoints {
            let oldest = metadata_list.get(0).unwrap();
            env.storage()
                .persistent()
                .remove(&ConfigDataKey::ChkpntSnap(oldest.checkpoint_id));
            metadata_list.remove(0);
            pruned += 1;
        }

        if pruned > 0 {
            env.storage()
                .instance()
                .set(&keys::CHECKPOINT_METADATA_LIST, &metadata_list);
            env.events().publish((symbol_short!("chkprune"),), pruned);
        }
    }

    fn get_checkpoint_or_panic(env: &Env, checkpoint_id: u64) -> CheckpointSnapshot {
        env.storage()
            .persistent()
            .get(&ConfigDataKey::ChkpntSnap(checkpoint_id))
            .unwrap_or_else(|| panic!("checkpoint not found"))
    }

    fn compute_checkpoint_hash(env: &Env, snapshot: &CheckpointSnapshot) -> BytesN<32> {
        let mut data = Bytes::new(env);
        Self::append_u32(&mut data, snapshot.format_version);
        Self::append_u32(&mut data, snapshot.health_weights.liquidity_weight);
        Self::append_u32(&mut data, snapshot.health_weights.price_stability_weight);
        Self::append_u32(&mut data, snapshot.health_weights.bridge_uptime_weight);
        Self::append_u32(&mut data, snapshot.health_weights.version);
        Self::append_u32(&mut data, snapshot.risk_score_config.health_weight_bps);
        Self::append_u32(&mut data, snapshot.risk_score_config.price_weight_bps);
        Self::append_u32(&mut data, snapshot.risk_score_config.volatility_weight_bps);
        Self::append_u32(&mut data, snapshot.risk_score_config.max_price_deviation_bps);
        Self::append_u32(&mut data, snapshot.risk_score_config.max_volatility_bps);
        Self::append_u32(&mut data, snapshot.risk_score_config.version);

        for asset_code in snapshot.monitored_assets.iter() {
            Self::append_string(&mut data, &asset_code);
        }

        for asset in snapshot.assets.iter() {
            Self::append_string(&mut data, &asset.asset_code);
            Self::append_asset_health(&mut data, &asset.health);
            Self::append_bool(&mut data, asset.has_latest_price);
            if asset.has_latest_price {
                Self::append_price_record(&mut data, &asset.latest_price);
            }
            Self::append_bool(&mut data, asset.has_health_result);
            if asset.has_health_result {
                Self::append_health_score_result(&mut data, &asset.health_result);
            }
        }

        env.crypto().sha256(&data).into()
    }

    fn build_checkpoint_comparison(
        env: &Env,
        from_snapshot: &CheckpointSnapshot,
        to_snapshot: &CheckpointSnapshot,
        from_checkpoint_id: u64,
        to_checkpoint_id: u64,
    ) -> CheckpointComparison {
        let mut added_assets = Vec::new(env);
        let mut removed_assets = Vec::new(env);
        let mut changed_assets = Vec::new(env);

        for to_asset in to_snapshot.assets.iter() {
            if let Some(from_asset) =
                Self::find_checkpoint_asset(&from_snapshot.assets, &to_asset.asset_code)
            {
                let health_changed = from_asset.health != to_asset.health;
                let price_changed = from_asset.latest_price != to_asset.latest_price;
                let health_result_changed = from_asset.health_result != to_asset.health_result;
                if health_changed || price_changed || health_result_changed {
                    changed_assets.push_back(CheckpointAssetDiff {
                        asset_code: to_asset.asset_code.clone(),
                        health_changed,
                        price_changed,
                        health_result_changed,
                    });
                }
            } else {
                added_assets.push_back(to_asset.asset_code.clone());
            }
        }

        for from_asset in from_snapshot.assets.iter() {
            if Self::find_checkpoint_asset(&to_snapshot.assets, &from_asset.asset_code).is_none() {
                removed_assets.push_back(from_asset.asset_code.clone());
            }
        }

        CheckpointComparison {
            from_checkpoint_id,
            to_checkpoint_id,
            timestamp_delta: to_snapshot
                .created_at
                .saturating_sub(from_snapshot.created_at),
            state_hash_changed: Self::compute_checkpoint_hash(env, from_snapshot)
                != Self::compute_checkpoint_hash(env, to_snapshot),
            weights_changed: from_snapshot.health_weights != to_snapshot.health_weights,
            added_assets,
            removed_assets,
            changed_assets,
        }
    }

    fn find_checkpoint_asset(
        assets: &Vec<CheckpointAssetState>,
        asset_code: &String,
    ) -> Option<CheckpointAssetState> {
        let mut i = 0;
        while i < assets.len() {
            let asset = assets.get(i).unwrap();
            if asset.asset_code == *asset_code {
                return Some(asset);
            }
            i += 1;
        }

        None
    }

    fn vec_contains_string(values: &Vec<String>, target: &String) -> bool {
        let mut i = 0;
        while i < values.len() {
            if values.get(i).unwrap() == *target {
                return true;
            }
            i += 1;
        }

        false
    }

    fn vec_contains_address(values: &Vec<Address>, target: &Address) -> bool {
        let mut i = 0;
        while i < values.len() {
            if values.get(i).unwrap() == *target {
                return true;
            }
            i += 1;
        }

        false
    }

    fn append_i128(buf: &mut Bytes, value: i128) {
        let bytes = value.to_be_bytes();
        let mut i = 0;
        while i < bytes.len() {
            buf.push_back(bytes[i]);
            i += 1;
        }
    }

    fn append_bool(buf: &mut Bytes, value: bool) {
        buf.push_back(if value { 1 } else { 0 });
    }

    fn append_string(buf: &mut Bytes, value: &String) {
        let raw = Self::str_to_bytes_inner(value.env(), value);
        Self::append_u32(buf, raw.len());
        buf.append(&raw);
    }

    /// Convert a `soroban_sdk::String` to `Bytes` by copying its content.
    fn str_to_bytes_inner(env: &Env, s: &String) -> Bytes {
        let len = s.len() as usize;
        // Use a fixed-size stack buffer; Soroban strings are bounded.
        // Max practical length is well under 256 bytes for our use cases.
        let mut buf = [0u8; 256];
        let safe_len = len.min(256);
        s.copy_into_slice(&mut buf[..safe_len]);
        let mut result = Bytes::new(env);
        let mut i = 0;
        while i < safe_len {
            result.push_back(buf[i]);
            i += 1;
        }
        result
    }

    #[allow(dead_code)]
    fn append_option_u64(buf: &mut Bytes, value: Option<u64>) {
        match value {
            Some(v) => {
                Self::append_bool(buf, true);
                Self::append_u64(buf, v);
            }
            None => Self::append_bool(buf, false),
        }
    }

    #[allow(dead_code)]
    fn append_checkpoint_trigger(buf: &mut Bytes, trigger: &CheckpointTrigger) {
        let code = match trigger {
            CheckpointTrigger::Automatic => 1u32,
            CheckpointTrigger::Manual => 2u32,
            CheckpointTrigger::Restore => 3u32,
        };
        Self::append_u32(buf, code);
    }

    fn append_asset_health(buf: &mut Bytes, health: &AssetHealth) {
        Self::append_string(buf, &health.asset_code);
        Self::append_u32(buf, health.health_score);
        Self::append_u32(buf, health.liquidity_score);
        Self::append_u32(buf, health.price_stability_score);
        Self::append_u32(buf, health.bridge_uptime_score);
        Self::append_bool(buf, health.paused);
        Self::append_bool(buf, health.active);
        Self::append_u64(buf, health.timestamp);
    }

    #[allow(dead_code)]
    fn append_option_price_record(buf: &mut Bytes, record: &Option<PriceRecord>) {
        match record {
            Some(price) => {
                Self::append_bool(buf, true);
                Self::append_string(buf, &price.asset_code);
                Self::append_i128(buf, price.price);
                Self::append_string(buf, &price.source);
                Self::append_u64(buf, price.timestamp);
            }
            None => Self::append_bool(buf, false),
        }
    }

    fn append_price_record(buf: &mut Bytes, price: &PriceRecord) {
        Self::append_string(buf, &price.asset_code);
        Self::append_i128(buf, price.price);
        Self::append_string(buf, &price.source);
        Self::append_u64(buf, price.timestamp);
    }

    #[allow(dead_code)]
    fn append_option_health_score_result(buf: &mut Bytes, result: &Option<HealthScoreResult>) {
        match result {
            Some(value) => {
                Self::append_bool(buf, true);
                Self::append_u32(buf, value.composite_score);
                Self::append_u32(buf, value.liquidity_score);
                Self::append_u32(buf, value.price_stability_score);
                Self::append_u32(buf, value.bridge_uptime_score);
                Self::append_u32(buf, value.weights.liquidity_weight);
                Self::append_u32(buf, value.weights.price_stability_weight);
                Self::append_u32(buf, value.weights.bridge_uptime_weight);
                Self::append_u32(buf, value.weights.version);
                Self::append_u64(buf, value.timestamp);
            }
            None => Self::append_bool(buf, false),
        }
    }

    fn append_health_score_result(buf: &mut Bytes, value: &HealthScoreResult) {
        Self::append_u32(buf, value.composite_score);
        Self::append_u32(buf, value.liquidity_score);
        Self::append_u32(buf, value.price_stability_score);
        Self::append_u32(buf, value.bridge_uptime_score);
        Self::append_u32(buf, value.weights.liquidity_weight);
        Self::append_u32(buf, value.weights.price_stability_weight);
        Self::append_u32(buf, value.weights.bridge_uptime_weight);
        Self::append_u32(buf, value.weights.version);
        Self::append_u64(buf, value.timestamp);
    }

    /// Load stored health weights or return defaults (30 / 40 / 30, v1).
    fn load_health_weights(env: &Env) -> HealthWeights {
        env.storage()
            .instance()
            .get(&keys::HEALTH_WEIGHTS)
            .unwrap_or(HealthWeights {
                liquidity_weight: 30,
                price_stability_weight: 40,
                bridge_uptime_weight: 30,
                version: 1,
            })
    }

    fn load_risk_score_config(env: &Env) -> RiskScoreConfig {
        env.storage()
            .instance()
            .get(&keys::RISK_SCORE_CONFIG)
            .unwrap_or_else(Self::default_risk_score_config)
    }

    /// Validate that three weights are each ≤ 100 and sum to exactly 100.
    fn validate_weights(liq: u32, stab: u32, up: u32) {
        if liq > 100 || stab > 100 || up > 100 {
            panic!("each weight must be between 0 and 100");
        }
        if liq + stab + up != 100 {
            panic!("weights must sum to 100");
        }
    }

    /// Validate that a single score is within the 0–100 range.
    fn validate_score_range(score: u32, name: &str) {
        if score > 100 {
            panic!("{} must be between 0 and 100", name);
        }
    }

    fn validate_risk_score_config(
        health_weight_bps: u32,
        price_weight_bps: u32,
        volatility_weight_bps: u32,
        max_price_deviation_bps: u32,
        max_volatility_bps: u32,
        version: u32,
    ) {
        if health_weight_bps > 10_000
            || price_weight_bps > 10_000
            || volatility_weight_bps > 10_000
        {
            panic!("risk weights must be between 0 and 10000");
        }
        if health_weight_bps + price_weight_bps + volatility_weight_bps != 10_000 {
            panic!("risk weights must sum to 10000");
        }
        if max_price_deviation_bps == 0 {
            panic!("max_price_deviation_bps must be greater than zero");
        }
        if max_volatility_bps == 0 {
            panic!("max_volatility_bps must be greater than zero");
        }
        if version == 0 {
            panic!("risk score config version must be greater than 0");
        }
    }

    /// Compute the weighted-average composite score.
    ///
    /// `composite = (liq * liq_w + stab * stab_w + up * up_w) / 100`
    fn compute_composite(
        liquidity_score: u32,
        price_stability_score: u32,
        bridge_uptime_score: u32,
        weights: &HealthWeights,
    ) -> u32 {
        let weighted_sum = (liquidity_score as u64) * (weights.liquidity_weight as u64)
            + (price_stability_score as u64) * (weights.price_stability_weight as u64)
            + (bridge_uptime_score as u64) * (weights.bridge_uptime_weight as u64);
        (weighted_sum / 100) as u32
    }

    fn build_risk_score_result(
        env: &Env,
        health_score: u32,
        price_deviation_bps: u32,
        volatility_bps: u32,
    ) -> RiskScoreResult {
        let config = Self::load_risk_score_config(env);
        let normalized_health_risk_bps = (100u32.saturating_sub(health_score)) * 100;
        let normalized_price_risk_bps =
            Self::normalize_signal_to_bps(price_deviation_bps, config.max_price_deviation_bps);
        let normalized_volatility_risk_bps =
            Self::normalize_signal_to_bps(volatility_bps, config.max_volatility_bps);

        let weighted_sum = (normalized_health_risk_bps as u64)
            * (config.health_weight_bps as u64)
            + (normalized_price_risk_bps as u64) * (config.price_weight_bps as u64)
            + (normalized_volatility_risk_bps as u64) * (config.volatility_weight_bps as u64);
        let risk_score_bps = Self::clamp_bps_u64(weighted_sum / 10_000);

        RiskScoreResult {
            risk_score_bps,
            normalized_health_risk_bps,
            normalized_price_risk_bps,
            normalized_volatility_risk_bps,
            health_score,
            price_deviation_bps,
            volatility_bps,
            config,
            timestamp: env.ledger().timestamp(),
        }
    }

    fn normalize_signal_to_bps(raw_signal_bps: u32, max_signal_bps: u32) -> u32 {
        let clamped_signal = if raw_signal_bps > max_signal_bps {
            max_signal_bps
        } else {
            raw_signal_bps
        };

        ((clamped_signal as u64) * 10_000 / (max_signal_bps as u64)) as u32
    }

    fn clamp_bps_u64(value: u64) -> u32 {
        if value > 10_000 {
            10_000
        } else {
            value as u32
        }
    }

    fn clamp_i128_to_u32(value: i128) -> u32 {
        if value <= 0 {
            0
        } else if value > u32::MAX as i128 {
            u32::MAX
        } else {
            value as u32
        }
    }

    fn stat_period_secs(period: &StatPeriod) -> u64 {
        match period {
            StatPeriod::Hour => 3_600,
            StatPeriod::Day => 86_400,
            StatPeriod::Week => 604_800,
            StatPeriod::Month => 2_592_000,
        }
    }

    fn collect_prices_for_period(env: &Env, asset_code: &String, period_secs: u64) -> Vec<i128> {
        let history: Vec<PriceRecord> = env
            .storage()
            .persistent()
            .get(&AssetDataKey::PriceHist(asset_code.clone()))
            .unwrap_or_else(|| Vec::new(env));
        let now = env.ledger().timestamp();
        let start_time = now.saturating_sub(period_secs);
        let mut prices: Vec<i128> = Vec::new(env);

        for record in history.iter() {
            if record.timestamp >= start_time && record.timestamp <= now {
                prices.push_back(record.price);
            }
        }

        prices
    }

    fn calculate_latest_price_deviation_bps(env: Env, prices: Vec<i128>) -> u32 {
        if prices.is_empty() {
            return 0;
        }

        let average_price = Self::calculate_average(env, prices.clone());
        if average_price <= 0 {
            return 0;
        }

        let latest_price = prices.get(prices.len() - 1).unwrap();
        let diff = if latest_price > average_price {
            latest_price - average_price
        } else {
            average_price - latest_price
        };

        Self::clamp_i128_to_u32((diff * 10_000) / average_price)
    }

    // -----------------------------------------------------------------------
    // Statistical Calculations (issue #133)
    // -----------------------------------------------------------------------

    /// Calculate simple moving average of a value series.
    ///
    /// Returns the arithmetic mean of the provided values.
    /// Gas-efficient implementation for on-chain calculations.
    pub fn calculate_average(_env: Env, values: Vec<i128>) -> i128 {
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
    pub fn volume_weighted_avg(_env: Env, values: Vec<i128>, volumes: Vec<i128>) -> i128 {
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
            weighted_sum = weighted_sum
                .checked_add(value * volume)
                .unwrap_or(weighted_sum);
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
            sum_squared_diff = sum_squared_diff
                .checked_add(diff * diff)
                .unwrap_or(sum_squared_diff);
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
        let annualization_factor =
            Self::integer_sqrt((SECONDS_PER_YEAR as i128 * 10_000) / period_secs as i128);

        // Annualized volatility
        (stddev_returns * annualization_factor) / 100
    }

    /// Calculate min and max values in a series.
    pub fn calculate_min_max(_env: Env, values: Vec<i128>) -> (i128, i128) {
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
    pub fn calculate_median(env: Env, values: Vec<i128>) -> i128 {
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
        let admin: Address = env.storage().instance().get(&keys::ADMIN).unwrap();
        if caller != admin {
            panic!("only admin can compute statistics");
        }

        // Determine time range based on period
        let now = env.ledger().timestamp();
        let period_secs = Self::stat_period_secs(&period);
        let start_time = now.saturating_sub(period_secs);

        // Get price history for the period
        let history: Vec<PriceRecord> = env
            .storage()
            .persistent()
            .get(&AssetDataKey::PriceHist(asset_code.clone()))
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
            period: period.clone(),
            timestamp: now,
            health_avg: 0,
            liquidity_avg: 0,
            price_volatility: volatility as u32,
            bridge_uptime: 0,
        };

        // Store in history
        let mut stats_history: Vec<Statistics> = env
            .storage()
            .persistent()
            .get(&AssetDataKey::Stats(asset_code.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        stats_history.push_back(stats.clone());
        env.storage()
            .persistent()
            .set(&AssetDataKey::Stats(asset_code.clone()), &stats_history);

        // Emit event
        env.events().publish(
            (symbol_short!("stats_avg"), asset_code.clone(), period),
            average,
        );

        stats
    }

    /// Get pre-computed statistics for an asset.
    ///
    /// Returns the most recent statistics for the specified period, or None
    /// if no statistics have been computed.
    pub fn get_statistics(env: Env, asset_code: String, period: StatPeriod) -> Option<Statistics> {
        let stats_history: Vec<Statistics> = env
            .storage()
            .persistent()
            .get(&AssetDataKey::Stats(asset_code.clone()))
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
            .get(&AssetDataKey::Stats(asset_code.clone()))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Trigger periodic statistics calculation for all active assets.
    ///
    /// Intended to be called periodically (e.g., by an automation service)
    /// to keep statistics up-to-date. Calculates daily statistics for all
    /// assets with sufficient data.
    pub fn trigger_periodic_stats(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&keys::ADMIN).unwrap();
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
                .get(&AssetDataKey::PriceHist(asset_code.clone()))
                .unwrap_or_else(|| Vec::new(&env));

            if history.len() < 2 {
                continue;
            }

            // Check last stats computation time
            let existing_stats =
                Self::get_statistics(env.clone(), asset_code.clone(), StatPeriod::Day);
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
                        sorted.set(j, prev);
                        j -= 1;
                    } else {
                        break;
                    }
                }
                sorted.set(j, key);
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
    pub fn calculate_ema(_env: Env, values: Vec<i128>, smoothing_factor: i128) -> i128 {
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
    pub fn get_stats_methods_docs(env: Env) -> String {
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
            13. trigger_periodic_stats() - Trigger batch computation",
        )
    }

    // -----------------------------------------------------------------------
    // Emergency Recovery (issue #298)
    // -----------------------------------------------------------------------

    /// Enter emergency recovery mode.
    ///
    /// Signals that the contract is in a degraded state and operators must
    /// follow a manual recovery runbook. Only the contract admin may activate
    /// recovery. The reason is stored on-chain for the audit trail.
    ///
    /// # Panics
    /// - `caller` is not the contract admin.
    /// - Recovery mode is already active.
    pub fn enter_recovery_mode(env: Env, caller: Address, reason: String) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&keys::ADMIN).unwrap();
        if caller != admin {
            panic!("only admin can enter recovery mode");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&keys::RECOVERY_MODE)
            .unwrap_or(false)
        {
            panic!("recovery mode already active");
        }
        let now = env.ledger().timestamp();
        env.storage().instance().set(&keys::RECOVERY_MODE, &true);
        env.storage().instance().set(&keys::RECOVERY_REASON, &reason);
        env.storage()
            .instance()
            .set(&keys::RECOVERY_ENTERED_AT, &now);
        env.storage()
            .instance()
            .set(&keys::RECOVERY_ENTERED_BY, &caller);
        // Reset step log for this recovery session
        let steps: Vec<RecoveryStep> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&keys::RECOVERY_STEPS, &steps);
        env.events()
            .publish((symbol_short!("rec_entr"), caller.clone()), (reason.clone(), now));
        Self::append_replay_event(
            &env,
            String::from_str(&env, "rec_entr"),
            caller.clone(),
            reason.clone(),
            0,
        );
        Self::append_admin_activity(&env, AdminActivityAction::RecoveryEntered, caller, reason);
    }

    /// Exit emergency recovery mode, returning the contract to normal operation.
    ///
    /// # Panics
    /// - `caller` is not the contract admin.
    /// - Recovery mode is not currently active.
    pub fn exit_recovery_mode(env: Env, caller: Address) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&keys::ADMIN).unwrap();
        if caller != admin {
            panic!("only admin can exit recovery mode");
        }
        if !env
            .storage()
            .instance()
            .get::<_, bool>(&keys::RECOVERY_MODE)
            .unwrap_or(false)
        {
            panic!("recovery mode is not active");
        }
        let now = env.ledger().timestamp();
        env.storage().instance().set(&keys::RECOVERY_MODE, &false);
        env.events()
            .publish((symbol_short!("rec_exit"), caller.clone()), now);
        Self::append_admin_activity(
            &env,
            AdminActivityAction::RecoveryExited,
            caller,
            String::from_str(&env, "recovery mode ended"),
        );
    }

    /// Append a completed recovery step to the on-chain audit trail.
    ///
    /// Steps are immutable once written and serve as an ordered record of
    /// actions taken during the recovery session. Capped at 50 steps per
    /// session (reset when recovery mode is re-entered).
    ///
    /// # Panics
    /// - `caller` is not the contract admin.
    /// - Recovery mode is not currently active.
    /// - The step log is already at 50 entries.
    pub fn record_recovery_step(env: Env, caller: Address, description: String) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&keys::ADMIN).unwrap();
        if caller != admin {
            panic!("only admin can record recovery steps");
        }
        if !env
            .storage()
            .instance()
            .get::<_, bool>(&keys::RECOVERY_MODE)
            .unwrap_or(false)
        {
            panic!("recovery mode is not active");
        }
        let mut steps: Vec<RecoveryStep> = env
            .storage()
            .persistent()
            .get(&keys::RECOVERY_STEPS)
            .unwrap_or_else(|| Vec::new(&env));
        if steps.len() >= 50 {
            panic!("recovery step log is full (max 50)");
        }
        let step = RecoveryStep {
            description,
            completed: true,
            recorded_at: env.ledger().timestamp(),
            actor: caller,
        };
        steps.push_back(step);
        env.storage()
            .persistent()
            .set(&keys::RECOVERY_STEPS, &steps);
    }

    /// Return a summary of the current recovery state.
    ///
    /// When recovery is not active, `active` is `false` and the `reason`,
    /// `entered_at`, and `entered_by` fields reflect the last recovery session
    /// (or zero-values if recovery has never been used).
    pub fn get_recovery_state(env: Env) -> RecoveryState {
        let active = env
            .storage()
            .instance()
            .get::<_, bool>(&keys::RECOVERY_MODE)
            .unwrap_or(false);
        let steps: Vec<RecoveryStep> = env
            .storage()
            .persistent()
            .get(&keys::RECOVERY_STEPS)
            .unwrap_or_else(|| Vec::new(&env));
        let reason: String = env
            .storage()
            .instance()
            .get(&keys::RECOVERY_REASON)
            .unwrap_or_else(|| String::from_str(&env, ""));
        let entered_at: u64 = env
            .storage()
            .instance()
            .get(&keys::RECOVERY_ENTERED_AT)
            .unwrap_or(0);
        let entered_by: Address = env
            .storage()
            .instance()
            .get(&keys::RECOVERY_ENTERED_BY)
            .unwrap_or_else(|| env.current_contract_address());
        RecoveryState {
            active,
            reason,
            entered_at,
            entered_by,
            step_count: steps.len(),
        }
    }

    /// Return the ordered list of recovery steps recorded in the current session.
    pub fn get_recovery_steps(env: Env) -> Vec<RecoveryStep> {
        env.storage()
            .persistent()
            .get(&keys::RECOVERY_STEPS)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Admin Activity Service (issue #299)
    // -----------------------------------------------------------------------

    /// Retrieve a page of admin activity log entries (oldest-first).
    ///
    /// Returns up to `limit` entries starting at zero-indexed `offset`.
    /// Maximum `limit` per call is 50.
    pub fn get_admin_activity(env: Env, limit: u32, offset: u32) -> AdminActivityPage {
        if limit > 50 {
            panic!("limit must not exceed 50");
        }
        let log: Vec<AdminActivityEntry> = env
            .storage()
            .persistent()
            .get(&keys::ADMIN_ACTIVITY_LOG)
            .unwrap_or_else(|| Vec::new(&env));
        let total = log.len();
        let mut page: Vec<AdminActivityEntry> = Vec::new(&env);
        let end = if offset + limit < total {
            offset + limit
        } else {
            total
        };
        for i in offset..end {
            page.push_back(log.get(i).unwrap());
        }
        AdminActivityPage { entries: page, total }
    }

    /// Retrieve admin activity entries for a specific actor (most-recent first).
    /// Returns up to `limit` matching entries; maximum is 50.
    pub fn get_admin_activity_by_actor(
        env: Env,
        actor: Address,
        limit: u32,
    ) -> Vec<AdminActivityEntry> {
        if limit > 50 {
            panic!("limit must not exceed 50");
        }
        let log: Vec<AdminActivityEntry> = env
            .storage()
            .persistent()
            .get(&keys::ADMIN_ACTIVITY_LOG)
            .unwrap_or_else(|| Vec::new(&env));
        let mut result: Vec<AdminActivityEntry> = Vec::new(&env);
        let len = log.len();
        let mut i = len;
        while i > 0 && result.len() < limit {
            i -= 1;
            let entry = log.get(i).unwrap();
            if entry.actor == actor {
                result.push_back(entry);
            }
        }
        result
    }

    /// Internal: append one entry to the admin activity log.
    /// Capped at 500 entries; oldest entries are trimmed when the cap is hit.
    fn append_admin_activity(
        env: &Env,
        action: AdminActivityAction,
        actor: Address,
        detail: String,
    ) {
        let seq: u32 = env
            .storage()
            .instance()
            .get(&keys::ADMIN_ACTIVITY_CTR)
            .unwrap_or(0u32)
            + 1;
        env.storage()
            .instance()
            .set(&keys::ADMIN_ACTIVITY_CTR, &seq);
        let entry = AdminActivityEntry {
            sequence: seq,
            action: action.clone(),
            actor,
            detail,
            timestamp: env.ledger().timestamp(),
        };
        let mut log: Vec<AdminActivityEntry> = env
            .storage()
            .persistent()
            .get(&keys::ADMIN_ACTIVITY_LOG)
            .unwrap_or_else(|| Vec::new(env));
        log.push_back(entry);
        if log.len() > 500 {
            let mut trimmed: Vec<AdminActivityEntry> = Vec::new(env);
            for i in 1..log.len() {
                trimmed.push_back(log.get(i).unwrap());
            }
            log = trimmed;
        }
        env.storage()
            .persistent()
            .set(&keys::ADMIN_ACTIVITY_LOG, &log);
        env.events()
            .publish((symbol_short!("adm_act"),), (seq, action));
    }

    // -----------------------------------------------------------------------
    // Multi-Source Health Submission (issue #300)
    // -----------------------------------------------------------------------

    /// Register a trusted health data source.
    ///
    /// Only the contract admin may register sources. `weight_bps` expresses the
    /// source's relative influence in basis points (10 000 = 100 %). Multiple
    /// sources need not sum to 10 000 — the aggregation normalises by total
    /// weight of contributing sources.
    ///
    /// # Panics
    /// - `caller` is not the contract admin.
    /// - `weight_bps` is zero.
    pub fn register_health_source(
        env: Env,
        caller: Address,
        source_id: String,
        weight_bps: u32,
    ) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&keys::ADMIN).unwrap();
        if caller != admin {
            panic!("only admin can register health sources");
        }
        if weight_bps == 0 {
            panic!("weight_bps must be greater than zero");
        }
        let now = env.ledger().timestamp();
        let source = HealthSource {
            source_id: source_id.clone(),
            weight_bps,
            trusted: true,
            registered_at: now,
        };
        let mut sources: Vec<HealthSource> = env
            .storage()
            .instance()
            .get(&keys::HEALTH_SOURCES)
            .unwrap_or_else(|| Vec::new(&env));
        // Replace if already registered, otherwise append
        let mut found = false;
        let mut updated: Vec<HealthSource> = Vec::new(&env);
        for s in sources.iter() {
            if s.source_id == source_id {
                updated.push_back(source.clone());
                found = true;
            } else {
                updated.push_back(s);
            }
        }
        if !found {
            updated.push_back(source);
        }
        env.storage().instance().set(&keys::HEALTH_SOURCES, &updated);
        env.events()
            .publish((symbol_short!("src_reg"), caller.clone()), source_id.clone());
        Self::append_admin_activity(
            &env,
            AdminActivityAction::AssetRegistered,
            caller,
            source_id,
        );
    }

    /// Revoke trust for a health source (it can no longer submit data).
    ///
    /// # Panics
    /// - `caller` is not the contract admin.
    /// - Source with `source_id` is not registered.
    pub fn revoke_health_source(env: Env, caller: Address, source_id: String) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&keys::ADMIN).unwrap();
        if caller != admin {
            panic!("only admin can revoke health sources");
        }
        let mut sources: Vec<HealthSource> = env
            .storage()
            .instance()
            .get(&keys::HEALTH_SOURCES)
            .unwrap_or_else(|| Vec::new(&env));
        let mut found = false;
        let mut updated: Vec<HealthSource> = Vec::new(&env);
        for s in sources.iter() {
            if s.source_id == source_id {
                let mut revoked = s.clone();
                revoked.trusted = false;
                updated.push_back(revoked);
                found = true;
            } else {
                updated.push_back(s);
            }
        }
        if !found {
            panic!("health source not registered");
        }
        env.storage().instance().set(&keys::HEALTH_SOURCES, &updated);
        env.events()
            .publish((symbol_short!("src_rev"), caller), source_id);
    }

    /// Submit health data from a named trusted source.
    ///
    /// The source must be registered and trusted (see `register_health_source`).
    /// Each source keeps its own per-asset entry; `get_aggregated_health` then
    /// combines all trusted sources into a weighted-average view.
    ///
    /// # Panics
    /// - `caller` is not the contract admin or a HealthSubmitter.
    /// - `source_id` is not a registered trusted source.
    pub fn submit_health_multi_source(
        env: Env,
        caller: Address,
        source_id: String,
        asset_code: String,
        health_score: u32,
        liquidity_score: u32,
        price_stability_score: u32,
        bridge_uptime_score: u32,
    ) {
        Self::assert_not_globally_paused(&env);
        Self::check_permission(&env, &caller, AdminRole::HealthSubmitter);
        // Verify this source is registered and trusted
        let sources: Vec<HealthSource> = env
            .storage()
            .instance()
            .get(&keys::HEALTH_SOURCES)
            .unwrap_or_else(|| Vec::new(&env));
        let mut is_trusted = false;
        for s in sources.iter() {
            if s.source_id == source_id && s.trusted {
                is_trusted = true;
                break;
            }
        }
        if !is_trusted {
            panic!("source is not registered or not trusted");
        }
        let now = env.ledger().timestamp();
        let entry = SourcedHealthEntry {
            source_id: source_id.clone(),
            asset_code: asset_code.clone(),
            health_score,
            liquidity_score,
            price_stability_score,
            bridge_uptime_score,
            submitted_at: now,
        };
        env.storage()
            .persistent()
            .set(&HealthSourceDataKey::Entry(source_id.clone(), asset_code.clone()), &entry);
        env.events().publish(
            (symbol_short!("ms_hlth"), caller.clone(), asset_code.clone()),
            (source_id.clone(), health_score, now),
        );
        Self::append_replay_event(
            &env,
            String::from_str(&env, "ms_hlth"),
            caller.clone(),
            asset_code.clone(),
            health_score as i128,
        );
        Self::append_admin_activity(
            &env,
            AdminActivityAction::HealthSubmitted,
            caller,
            asset_code,
        );
    }

    /// Compute a weighted-average health view for an asset across all trusted sources.
    ///
    /// Sources with no entry for `asset_code` are skipped. Returns `None` if no
    /// trusted source has submitted data for the asset.
    pub fn get_aggregated_health(env: Env, asset_code: String) -> Option<AggregatedHealth> {
        let sources: Vec<HealthSource> = env
            .storage()
            .instance()
            .get(&keys::HEALTH_SOURCES)
            .unwrap_or_else(|| Vec::new(&env));

        let mut total_weight: u64 = 0;
        let mut weighted_health: u64 = 0;
        let mut weighted_liquidity: u64 = 0;
        let mut weighted_price: u64 = 0;
        let mut weighted_uptime: u64 = 0;
        let mut count: u32 = 0;

        for source in sources.iter() {
            if !source.trusted {
                continue;
            }
            let key = HealthSourceDataKey::Entry(source.source_id.clone(), asset_code.clone());
            let entry: Option<SourcedHealthEntry> = env.storage().persistent().get(&key);
            if let Some(e) = entry {
                let w = source.weight_bps as u64;
                total_weight += w;
                weighted_health += w * e.health_score as u64;
                weighted_liquidity += w * e.liquidity_score as u64;
                weighted_price += w * e.price_stability_score as u64;
                weighted_uptime += w * e.bridge_uptime_score as u64;
                count += 1;
            }
        }

        if count == 0 || total_weight == 0 {
            return None;
        }

        Some(AggregatedHealth {
            asset_code,
            weighted_health_score: (weighted_health / total_weight) as u32,
            weighted_liquidity_score: (weighted_liquidity / total_weight) as u32,
            weighted_price_stability_score: (weighted_price / total_weight) as u32,
            weighted_bridge_uptime_score: (weighted_uptime / total_weight) as u32,
            source_count: count,
            computed_at: env.ledger().timestamp(),
        })
    }

    /// Return the list of all registered health sources.
    pub fn get_health_sources(env: Env) -> Vec<HealthSource> {
        env.storage()
            .instance()
            .get(&keys::HEALTH_SOURCES)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Event Replay Helpers (issue #296)
    // -----------------------------------------------------------------------

    /// Return the current event payload schema version.
    ///
    /// Off-chain consumers should call this after connecting to detect whether
    /// a schema migration has occurred and rebuild their replay state if needed.
    pub fn get_replay_schema_version(_env: Env) -> u32 {
        EVENT_SCHEMA_VERSION
    }

    /// Query replay-friendly event history ordered by ascending `ordering_key`.
    ///
    /// Returns up to `limit` entries whose `ordering_key` is ≥
    /// `from_ordering_key`. Pass `0` to start from the beginning of the log.
    /// Maximum `limit` per call is 100. The returned `EventReplayPage` includes
    /// the total log size so callers can implement cursor-based pagination.
    pub fn get_replay_events(env: Env, from_ordering_key: u64, limit: u32) -> EventReplayPage {
        if limit > 100 {
            panic!("limit must not exceed 100");
        }
        let log: Vec<EventReplayEntry> = env
            .storage()
            .persistent()
            .get(&keys::EVENT_REPLAY_LOG)
            .unwrap_or_else(|| Vec::new(&env));
        let total = log.len();
        let mut page: Vec<EventReplayEntry> = Vec::new(&env);
        for i in 0..total {
            if page.len() >= limit {
                break;
            }
            let entry = log.get(i).unwrap();
            if entry.ordering_key >= from_ordering_key {
                page.push_back(entry);
            }
        }
        EventReplayPage {
            entries: page,
            total,
            schema_version: EVENT_SCHEMA_VERSION,
        }
    }

    /// Return the total number of entries in the event replay log.
    pub fn get_replay_log_size(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get::<_, Vec<EventReplayEntry>>(&keys::EVENT_REPLAY_LOG)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Internal: append one entry to the event replay log.
    ///
    /// The `ordering_key` is `(timestamp << 32) | (sequence & 0xFFFF_FFFF)`
    /// providing stable, deterministic ordering even for same-timestamp events.
    /// Log is capped at 1 000 entries; oldest entries are trimmed on overflow.
    fn append_replay_event(
        env: &Env,
        event_type: String,
        actor: Address,
        subject: String,
        value: i128,
    ) {
        let seq: u32 = env
            .storage()
            .instance()
            .get(&keys::EVENT_REPLAY_CTR)
            .unwrap_or(0u32)
            + 1;
        env.storage().instance().set(&keys::EVENT_REPLAY_CTR, &seq);
        let now = env.ledger().timestamp();
        let ordering_key = (now << 32) | (seq as u64);
        let entry = EventReplayEntry {
            event_id: seq,
            event_type,
            actor,
            subject,
            value,
            timestamp: now,
            ordering_key,
            schema_version: EVENT_SCHEMA_VERSION,
        };
        let mut log: Vec<EventReplayEntry> = env
            .storage()
            .persistent()
            .get(&keys::EVENT_REPLAY_LOG)
            .unwrap_or_else(|| Vec::new(env));
        log.push_back(entry);
        if log.len() > 1000 {
            let mut trimmed: Vec<EventReplayEntry> = Vec::new(env);
            for i in 1..log.len() {
                trimmed.push_back(log.get(i).unwrap());
            }
            log = trimmed;
        }
        env.storage()
            .persistent()
            .set(&keys::EVENT_REPLAY_LOG, &log);
    }

    // ── Trusted Source Registry ───────────────────────────────────────────────

    /// Register a new trusted source for contract submissions.
    ///
    /// Only admin or super admin can register sources. Trusted sources are
    /// authorized to submit health scores, price updates, and other contract data.
    ///
    /// # Arguments
    ///
    /// * `caller` - The admin performing the registration
    /// * `source_address` - The address to register as a trusted source
    /// * `name` - Human-readable name/description for the source
    ///
    /// # Panics
    ///
    /// * If `caller` is not an admin or super admin
    /// * If `name` is empty
    ///
    /// # Events
    ///
    /// Emits a `SourceRegisteredEvent` on success.
    ///
    /// # Example
    ///
    /// ```ignore
    /// contract.register_trusted_source(
    ///     env,
    ///     admin_address,
    ///     oracle_address,
    ///     "CoinGecko Price Oracle".into(),
    /// );
    /// ```
    pub fn register_trusted_source(
        env: Env,
        caller: Address,
        source_address: Address,
        name: String,
    ) {
        caller.require_auth();
        
        // Check admin permission
        let admin: Address = env
            .storage()
            .instance()
            .get(&keys::ADMIN)
            .unwrap_or_else(|| panic!("contract not initialized"));
        
        if caller != admin {
            acl::require_permission(&env, &caller, &admin, &Permission::ManageConfig);
        }

        source_trust::register_trusted_source(&env, &caller, &source_address, name);
    }

    /// Revoke a trusted source, preventing it from making further submissions.
    ///
    /// Only admin or super admin can revoke sources. The source record is
    /// preserved for audit purposes but marked as inactive.
    ///
    /// # Arguments
    ///
    /// * `caller` - The admin performing the revocation
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
    ///
    /// # Example
    ///
    /// ```ignore
    /// contract.revoke_trusted_source(env, admin_address, oracle_address);
    /// ```
    pub fn revoke_trusted_source(env: Env, caller: Address, source_address: Address) {
        caller.require_auth();
        
        // Check admin permission
        let admin: Address = env
            .storage()
            .instance()
            .get(&keys::ADMIN)
            .unwrap_or_else(|| panic!("contract not initialized"));
        
        if caller != admin {
            acl::require_permission(&env, &caller, &admin, &Permission::ManageConfig);
        }

        source_trust::revoke_trusted_source(&env, &caller, &source_address);
    }

    /// Check if an address is currently a trusted source.
    ///
    /// # Arguments
    ///
    /// * `source_address` - The address to check
    ///
    /// # Returns
    ///
    /// `true` if the address is registered and active, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let is_trusted = contract.is_trusted_source(env, oracle_address);
    /// if is_trusted {
    ///     // Allow submission
    /// }
    /// ```
    pub fn is_trusted_source(env: Env, source_address: Address) -> bool {
        source_trust::is_trusted_source(&env, &source_address)
    }

    /// Get detailed information about a trusted source.
    ///
    /// # Arguments
    ///
    /// * `source_address` - The address to query
    ///
    /// # Returns
    ///
    /// `Some(TrustedSource)` if the source is registered, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(source) = contract.get_trusted_source(env, oracle_address) {
    ///     log!("Source: {}, Active: {}", source.name, source.is_active);
    /// }
    /// ```
    pub fn get_trusted_source(
        env: Env,
        source_address: Address,
    ) -> Option<source_trust::TrustedSource> {
        source_trust::get_trusted_source(&env, &source_address)
    }

    /// Get a list of all registered trusted sources (active and revoked).
    ///
    /// # Returns
    ///
    /// A vector of `SourceInfo` records for all registered sources.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let all_sources = contract.get_all_trusted_sources(env);
    /// for source in all_sources.iter() {
    ///     log!("Source: {}, Active: {}", source.name, source.is_active);
    /// }
    /// ```
    pub fn get_all_trusted_sources(env: Env) -> Vec<source_trust::SourceInfo> {
        source_trust::get_all_trusted_sources(&env)
    }

    /// Get a list of only active trusted sources.
    ///
    /// # Returns
    ///
    /// A vector of `SourceInfo` records for active sources only.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let active_sources = contract.get_active_trusted_sources(env);
    /// log!("Active sources: {}", active_sources.len());
    /// ```
    pub fn get_active_trusted_sources(env: Env) -> Vec<source_trust::SourceInfo> {
        source_trust::get_active_trusted_sources(&env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::{Env, IntoVal};

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
    // Checkpoint tests (issue #105)
    // -----------------------------------------------------------------------

    #[test]
    fn test_manual_checkpoint_stores_snapshot_and_metadata() {
        let (env, client, admin) = setup();
        env.ledger().set_timestamp(100);
        client.set_checkpoint_config(&admin, &86_400, &10, &2);

        let usdc = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "oracle");
        let label = String::from_str(&env, "manual-baseline");

        client.register_asset(&admin, &usdc);
        client.submit_price(&admin, &usdc, &1_000_000, &source);

        let metadata = client.create_checkpoint(&admin, &label);
        assert_eq!(metadata.checkpoint_id, 2);
        assert_eq!(metadata.format_version, 2);
        assert_eq!(metadata.label, label);
        assert_eq!(metadata.asset_count, 1);
        assert_eq!(metadata.trigger, CheckpointTrigger::Manual);

        let snapshot = client.get_checkpoint(&metadata.checkpoint_id).unwrap();
        assert_eq!(snapshot.assets.len(), 1);
        assert_eq!(snapshot.health_weights.version, 1);
        assert_eq!(snapshot.assets.get(0).unwrap().asset_code, usdc);

        let validation = client.validate_checkpoint(&metadata.checkpoint_id);
        assert!(validation.is_valid);
    }

    #[test]
    fn test_compare_checkpoints_detects_asset_changes() {
        let (env, client, admin) = setup();
        client.set_checkpoint_config(&admin, &86_400, &10, &1);

        let usdc = String::from_str(&env, "USDC");
        let eurc = String::from_str(&env, "EURC");
        let source = String::from_str(&env, "oracle");

        env.ledger().set_timestamp(10);
        client.register_asset(&admin, &usdc);
        let first = client.create_checkpoint(&admin, &String::from_str(&env, "before"));

        env.ledger().set_timestamp(20);
        client.submit_price(&admin, &usdc, &1_020_000, &source);
        client.register_asset(&admin, &eurc);
        let second = client.create_checkpoint(&admin, &String::from_str(&env, "after"));

        let comparison = client.compare_checkpoints(&first.checkpoint_id, &second.checkpoint_id);
        assert!(comparison.state_hash_changed);
        assert_eq!(comparison.added_assets.len(), 1);
        assert_eq!(comparison.added_assets.get(0).unwrap(), eurc);
        assert_eq!(comparison.changed_assets.len(), 1);
        assert_eq!(comparison.changed_assets.get(0).unwrap().asset_code, usdc);
        assert!(comparison.changed_assets.get(0).unwrap().price_changed);
    }

    #[test]
    fn test_checkpoint_pruning_keeps_latest_entries() {
        let (env, client, admin) = setup();
        client.set_checkpoint_config(&admin, &0, &2, &1);
        let usdc = String::from_str(&env, "USDC");

        env.ledger().set_timestamp(1);
        client.register_asset(&admin, &usdc);

        env.ledger().set_timestamp(2);
        let second = client.create_checkpoint(&admin, &String::from_str(&env, "second"));

        env.ledger().set_timestamp(3);
        let third = client.create_checkpoint(&admin, &String::from_str(&env, "third"));

        let checkpoints = client.list_checkpoints();
        assert_eq!(checkpoints.len(), 2);
        assert_eq!(
            checkpoints.get(0).unwrap().checkpoint_id,
            second.checkpoint_id
        );
        assert_eq!(
            checkpoints.get(1).unwrap().checkpoint_id,
            third.checkpoint_id
        );
        assert!(client.get_checkpoint(&1).is_none());
    }

    #[test]
    fn test_auto_checkpoint_respects_interval() {
        let (env, client, admin) = setup();
        client.set_checkpoint_config(&admin, &60, &10, &1);
        let usdc = String::from_str(&env, "USDC");
        let source = String::from_str(&env, "oracle");

        env.ledger().set_timestamp(100);
        client.register_asset(&admin, &usdc);
        assert_eq!(client.list_checkpoints().len(), 1);

        env.ledger().set_timestamp(120);
        client.submit_price(&admin, &usdc, &1_000_000, &source);
        assert_eq!(client.list_checkpoints().len(), 1);

        env.ledger().set_timestamp(200);
        client.submit_health(&admin, &usdc, &80, &75, &90, &88);
        assert_eq!(client.list_checkpoints().len(), 2);
        assert_eq!(
            client.get_latest_checkpoint().unwrap().trigger,
            CheckpointTrigger::Automatic
        );
    }

    #[test]
    fn test_restore_from_checkpoint_restores_prior_state() {
        let (env, client, admin) = setup();
        client.set_checkpoint_config(&admin, &86_400, &10, &1);

        let usdc = String::from_str(&env, "USDC");
        let eurc = String::from_str(&env, "EURC");
        let source = String::from_str(&env, "oracle");

        env.ledger().set_timestamp(1_000);
        client.register_asset(&admin, &usdc);
        client.submit_price(&admin, &usdc, &1_000_000, &source);
        let baseline = client.create_checkpoint(&admin, &String::from_str(&env, "baseline"));

        env.ledger().set_timestamp(2_000);
        client.submit_health(&admin, &usdc, &91, &92, &93, &94);
        client.register_asset(&admin, &eurc);
        client.submit_price(&admin, &eurc, &990_000, &source);

        env.ledger().set_timestamp(3_000);
        let restore_meta = client.restore_from_checkpoint(&admin, &baseline.checkpoint_id);

        assert_eq!(client.get_monitored_assets().len(), 1);
        assert_eq!(client.get_monitored_assets().get(0).unwrap(), usdc);
        assert!(client.get_health(&eurc).is_none());
        assert_eq!(client.get_price(&usdc).unwrap().price, 1_000_000);
        assert_eq!(restore_meta.trigger, CheckpointTrigger::Restore);
        assert_eq!(restore_meta.restored_from, Some(baseline.checkpoint_id));
    }

    // -----------------------------------------------------------------------
    // Data retention and cleanup tests (issue #100)
    // -----------------------------------------------------------------------

    fn find_storage_entry(stats: &StorageStats, data_type: RetentionDataType) -> StorageUsageEntry {
        let mut i = 0;
        while i < stats.entries.len() {
            let entry = stats.entries.get(i).unwrap();
            if entry.data_type == data_type {
                return entry;
            }
            i += 1;
        }

        panic!("storage usage entry not found");
    }

    #[test]
    #[should_panic(expected = "only admin or SuperAdmin can manage retention policies")]
    fn test_set_retention_policy_requires_admin_or_super_admin() {
        let (env, client, _admin) = setup();
        let stranger = Address::generate(&env);

        client.set_retention_policy(
            &stranger,
            &RetentionDataType::SupplyMismatches,
            &86_400,
            &3_600,
            &25,
            &false,
            &true,
        );
    }

    #[test]
    fn test_cleanup_old_data_archives_and_preserves_latest_record() {
        let (env, client, admin) = setup();
        let bridge = String::from_str(&env, "CIRCLE_USDC");
        let asset = String::from_str(&env, "USDC");

        client.set_retention_policy(
            &admin,
            &RetentionDataType::SupplyMismatches,
            &100,
            &1_000_000,
            &20,
            &true,
            &true,
        );

        env.ledger().set_timestamp(100);
        client.record_supply_mismatch(&bridge, &asset, &1_000_000, &1_001_000);

        env.ledger().set_timestamp(200);
        client.record_supply_mismatch(&bridge, &asset, &1_000_000, &1_002_000);

    use super::*;

    use soroban_sdk::testutils::Address as _;

    use soroban_sdk::testutils::Events;

    use soroban_sdk::testutils::Ledger;

    use soroban_sdk::{Env, IntoVal};



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

    // Event emission tests (issue #29)

    // -----------------------------------------------------------------------



    /// Helper: verify that the contract emitted at least one event whose

    /// first topic matches the given symbol.

    fn assert_has_event(env: &Env, contract: &Address, expected_topic: soroban_sdk::Symbol) {

        let events = env.events().all();

        let mut found = false;

        for i in 0..events.len() {

            let (addr, topics, _data) = events.get(i).unwrap();
            if addr == *contract && topics.len() > 0 {
                // The first topic is the event symbol stored as a Val;

                // convert via IntoVal for comparison.

                let topic_val: soroban_sdk::Val = topics.get(0).unwrap();

                let expected_val: soroban_sdk::Val = expected_topic.into_val(env);

                if topic_val.get_payload() == expected_val.get_payload() {

                    found = true;

                    break;

                }

            }

        }

        assert!(found, "expected event with topic not found");

    }



    #[test]

    fn test_submit_health_emits_event() {

        let (env, client, admin) = setup();

        env.ledger().set_timestamp(1_000_000);

        let asset = String::from_str(&env, "USDC");



        client.register_asset(&admin, &asset);

        client.submit_health(&admin, &asset, &85, &90, &80, &75);



        assert_has_event(&env, &client.address, symbol_short!("health_up"));

    }



    #[test]

    fn test_submit_price_emits_event() {

        let (env, client, admin) = setup();

        env.ledger().set_timestamp(1_000_000);

        let asset = String::from_str(&env, "USDC");

        let source = String::from_str(&env, "Stellar DEX");



        client.register_asset(&admin, &asset);

        client.submit_price(&admin, &asset, &1_000_000, &source);



        assert_has_event(&env, &client.address, symbol_short!("price_up"));

    }

    #[test]

    fn test_check_price_deviation_emits_event_on_alert() {

        let (env, client, admin) = setup();

        env.ledger().set_timestamp(1_000_000);

        let asset = String::from_str(&env, "USDC");

        let source = String::from_str(&env, "Stellar DEX");



        client.register_asset(&admin, &asset);

        client.submit_price(&admin, &asset, &1_000_000, &source);



        // 15 % deviation → High severity triggers event

        let result = client.check_price_deviation(&asset, &1_150_000);

        assert!(result.is_some());



        assert_has_event(&env, &client.address, symbol_short!("price_dev"));

    }



    #[test]

    fn test_record_supply_mismatch_emits_event() {

        let (env, client, _admin) = setup();

        env.ledger().set_timestamp(1_000_000);



        let bridge = String::from_str(&env, "CIRCLE_USDC");

        let asset = String::from_str(&env, "USDC");



        client.record_supply_mismatch(&bridge, &asset, &1_000_000, &1_002_000);



        assert_has_event(&env, &client.address, symbol_short!("supply_mm"));

    }



    #[test]

    fn test_record_liquidity_depth_emits_event() {

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



        assert_has_event(&env, &client.address, symbol_short!("liq_chg"));

    }



    #[test]

    fn test_grant_role_emits_event() {

        let (env, client, admin) = setup();

        let submitter = Address::generate(&env);



        client.grant_role(&admin, &submitter, &AdminRole::HealthSubmitter);



        assert_has_event(&env, &client.address, symbol_short!("role_grnt"));

    }



    #[test]

    fn test_revoke_role_emits_event() {

        let (env, client, admin) = setup();

        let submitter = Address::generate(&env);



        client.grant_role(&admin, &submitter, &AdminRole::HealthSubmitter);

        client.revoke_role(&admin, &submitter, &AdminRole::HealthSubmitter);



        assert_has_event(&env, &client.address, symbol_short!("role_revk"));

    }



    #[test]

    fn test_set_deviation_threshold_emits_event() {

        let (env, client, _admin) = setup();

        let asset = String::from_str(&env, "USDC");



        client.set_deviation_threshold(&asset, &50, &100, &200);



        assert_has_event(&env, &client.address, symbol_short!("thresh_up"));

    }



    #[test]

    fn test_set_mismatch_threshold_emits_event() {

        let (env, client, _admin) = setup();



        client.set_mismatch_threshold(&5);



        assert_has_event(&env, &client.address, symbol_short!("thresh_up"));

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

    // Automated health score calculation tests (issue #26)

    // -----------------------------------------------------------------------



    #[test]

    fn test_get_health_weights_returns_defaults() {

        let (_env, client, _admin) = setup();

        let weights = client.get_health_weights();

        assert_eq!(weights.liquidity_weight, 30);

        assert_eq!(weights.price_stability_weight, 40);

        assert_eq!(weights.bridge_uptime_weight, 30);

        assert_eq!(weights.version, 1);

    }



    #[test]

    fn test_set_health_weights_stores_custom_weights() {

        let (_env, client, admin) = setup();

        client.set_health_weights(&admin, &20, &50, &30, &2);



        let weights = client.get_health_weights();

        assert_eq!(weights.liquidity_weight, 20);

        assert_eq!(weights.price_stability_weight, 50);

        assert_eq!(weights.bridge_uptime_weight, 30);

        assert_eq!(weights.version, 2);

    }



    #[test]

    #[should_panic]

    fn test_set_health_weights_rejects_non_admin() {

        let (env, client, _admin) = setup();

        let stranger = Address::generate(&env);

        client.set_health_weights(&stranger, &30, &40, &30, &1);

    }



    #[test]

    #[should_panic]

    fn test_set_health_weights_rejects_invalid_sum() {

        let (_env, client, admin) = setup();

        // Weights sum to 90, not 100

        client.set_health_weights(&admin, &30, &30, &30, &1);

    }



    #[test]

    #[should_panic]

    fn test_set_health_weights_rejects_weight_over_100() {

        let (_env, client, admin) = setup();

        client.set_health_weights(&admin, &110, &0, &0, &1);

    }



    #[test]

    #[should_panic]

    fn test_set_health_weights_rejects_zero_version() {

        let (_env, client, admin) = setup();

        client.set_health_weights(&admin, &30, &40, &30, &0);

    }



    #[test]

    fn test_calculate_health_score_default_weights() {

        let (env, client, _admin) = setup();

        env.ledger().set_timestamp(1_000_000);



        // liq=80, stab=90, up=70 → (80*30 + 90*40 + 70*30) / 100 = (2400+3600+2100)/100 = 81

        let result = client.calculate_health_score(&80, &90, &70);

        assert_eq!(result.composite_score, 81);

        assert_eq!(result.liquidity_score, 80);

        assert_eq!(result.price_stability_score, 90);

        assert_eq!(result.bridge_uptime_score, 70);

        assert_eq!(result.weights.liquidity_weight, 30);

        assert_eq!(result.weights.price_stability_weight, 40);

        assert_eq!(result.weights.bridge_uptime_weight, 30);

        assert_eq!(result.timestamp, 1_000_000);

    }



    #[test]

    fn test_calculate_health_score_custom_weights() {

        let (env, client, admin) = setup();

        env.ledger().set_timestamp(2_000_000);



        // Set custom weights: 50/30/20

        client.set_health_weights(&admin, &50, &30, &20, &2);



        // liq=60, stab=80, up=100 → (60*50 + 80*30 + 100*20) / 100 = (3000+2400+2000)/100 = 74

        let result = client.calculate_health_score(&60, &80, &100);

        assert_eq!(result.composite_score, 74);

        assert_eq!(result.weights.version, 2);

    }

    #[test]

    fn test_calculate_health_score_all_perfect() {

        let (_env, client, _admin) = setup();



        let result = client.calculate_health_score(&100, &100, &100);

        assert_eq!(result.composite_score, 100);

    }



    #[test]

    fn test_calculate_health_score_all_zero() {

        let (_env, client, _admin) = setup();



        let result = client.calculate_health_score(&0, &0, &0);

        assert_eq!(result.composite_score, 0);

    }



    #[test]

    #[should_panic]

    fn test_calculate_health_score_rejects_score_over_100() {

        let (_env, client, _admin) = setup();

        client.calculate_health_score(&101, &90, &80);

    }



    #[test]

    fn test_submit_calculated_health_stores_records() {

        let (env, client, admin) = setup();

        env.ledger().set_timestamp(3_000_000);



        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);



        client.submit_calculated_health(&admin, &usdc, &80, &90, &70, &None);



        // Check AssetHealth record

        let health = client.get_health(&usdc).unwrap();

        // (80*30 + 90*40 + 70*30) / 100 = 81

        assert_eq!(health.health_score, 81);

        assert_eq!(health.liquidity_score, 80);

        assert_eq!(health.price_stability_score, 90);

        assert_eq!(health.bridge_uptime_score, 70);

        assert_eq!(health.timestamp, 3_000_000);



        // Check HealthScoreResult record

        let result = client.get_health_score_result(&usdc).unwrap();

        assert_eq!(result.composite_score, 81);

        assert_eq!(result.weights.liquidity_weight, 30);

        assert_eq!(result.timestamp, 3_000_000);

    }



    #[test]

    fn test_submit_calculated_health_with_manual_override() {

        let (env, client, admin) = setup();

        env.ledger().set_timestamp(4_000_000);



        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);



        // Override with manual score of 95

        client.submit_calculated_health(&admin, &usdc, &80, &90, &70, &Some(95));



        // AssetHealth should have the overridden score

        let health = client.get_health(&usdc).unwrap();

        assert_eq!(health.health_score, 95);



        // HealthScoreResult should still have the calculated composite

        let result = client.get_health_score_result(&usdc).unwrap();

        assert_eq!(result.composite_score, 81);

    }



    #[test]

    #[should_panic]

    fn test_submit_calculated_health_rejects_override_over_100() {

        let (env, client, admin) = setup();

        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);

        client.submit_calculated_health(&admin, &usdc, &80, &90, &70, &Some(101));

    }



    #[test]

    #[should_panic]

    fn test_submit_calculated_health_rejects_unregistered_asset() {

        let (env, client, admin) = setup();

        let usdc = String::from_str(&env, "USDC");

        client.submit_calculated_health(&admin, &usdc, &80, &90, &70, &None);

    }



    #[test]

    #[should_panic]

    fn test_submit_calculated_health_rejects_paused_asset() {

        let (env, client, admin) = setup();

        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);

        client.pause_asset(&admin, &usdc);

        client.submit_calculated_health(&admin, &usdc, &80, &90, &70, &None);

    }



    #[test]

    #[should_panic]

    fn test_submit_calculated_health_rejects_unauthorized() {

        let (env, client, admin) = setup();

        let stranger = Address::generate(&env);

        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);

        client.submit_calculated_health(&stranger, &usdc, &80, &90, &70, &None);

    }



    #[test]

    fn test_submit_calculated_health_with_role() {

        let (env, client, admin) = setup();

        let submitter = Address::generate(&env);

        client.grant_role(&admin, &submitter, &AdminRole::HealthSubmitter);



        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);



        client.submit_calculated_health(&submitter, &usdc, &75, &85, &95, &None);



        let health = client.get_health(&usdc).unwrap();

        // (75*30 + 85*40 + 95*30) / 100 = (2250+3400+2850)/100 = 85

        assert_eq!(health.health_score, 85);

    }



    #[test]

    fn test_set_health_weights_by_super_admin() {

        let (env, client, admin) = setup();

        let super_admin = Address::generate(&env);

        client.grant_role(&admin, &super_admin, &AdminRole::SuperAdmin);



        client.set_health_weights(&super_admin, &40, &40, &20, &3);



        let weights = client.get_health_weights();

        assert_eq!(weights.liquidity_weight, 40);

        assert_eq!(weights.price_stability_weight, 40);

        assert_eq!(weights.bridge_uptime_weight, 20);

        assert_eq!(weights.version, 3);

    }



    #[test]

    fn test_get_health_score_result_returns_none_for_unknown_asset() {

        let (env, client, _admin) = setup();

        let unknown = String::from_str(&env, "UNKNOWN");

        assert!(client.get_health_score_result(&unknown).is_none());

    }



    #[test]

    fn test_submit_calculated_health_updates_on_second_call() {

        let (env, client, admin) = setup();

        let usdc = String::from_str(&env, "USDC");

        client.register_asset(&admin, &usdc);



        env.ledger().set_timestamp(1_000_000);

        client.submit_calculated_health(&admin, &usdc, &80, &90, &70, &None);

        let first = client.get_health(&usdc).unwrap();

        assert_eq!(first.health_score, 81);



        env.ledger().set_timestamp(2_000_000);

        client.submit_calculated_health(&admin, &usdc, &60, &70, &50, &None);

        let second = client.get_health(&usdc).unwrap();

        // (60*30 + 70*40 + 50*30) / 100 = (1800+2800+1500)/100 = 61

        assert_eq!(second.health_score, 61);

        assert_eq!(second.timestamp, 2_000_000);

    }



    #[test]

    fn test_calculate_health_score_edge_weights() {

        let (_env, client, admin) = setup();



        // Set weights to 0/100/0 — only price stability matters

        client.set_health_weights(&admin, &0, &100, &0, &4);



        let result = client.calculate_health_score(&0, &88, &0);

        assert_eq!(result.composite_score, 88);
    }
}
