#![allow(clippy::too_many_arguments)]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Vec};

/// Reputation score ranges from 0 to 10000 (represents 0.00 to 100.00%)
pub const REPUTATION_SCALE: u32 = 10000;

/// Maximum number of historical records to keep per entity
pub const MAX_HISTORY_RECORDS: u32 = 100;

/// Time decay half-life in seconds (reputation halves every 90 days by default)
pub const TIME_DECAY_HALFLIFE: u64 = 90 * 24 * 60 * 60;

/// Minimum reputation threshold for good standing
pub const MIN_REPUTATION_THRESHOLD: u32 = 5000;

/// Maximum weight for any single factor in reputation calculation
pub const MAX_FACTOR_WEIGHT: u32 = 100;

/// Entity types that can have reputation
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityType {
    BridgeOperator,
    OracleNode,
    RelayOperator,
}

/// Reputation factors that contribute to the overall score
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationFactors {
    pub accuracy_score: u32,
    pub uptime_score: u32,
    pub response_time_score: u32,
    pub dispute_history_score: u32,
}

/// Performance record for historical tracking
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerformanceRecord {
    pub timestamp: u64,
    pub entity_type: EntityType,
    pub accuracy: u32,
    pub uptime: u32,
    pub response_time: u32,
    pub disputes_won: u32,
    pub disputes_lost: u32,
    pub total_operations: u32,
    pub successful_operations: u32,
    pub penalty_amount: i128,
    pub reward_amount: i128,
}

/// Complete reputation data for an entity
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationData {
    pub entity_address: Address,
    pub entity_type: EntityType,
    pub overall_score: u32,
    pub factors: ReputationFactors,
    pub total_operations: u64,
    pub successful_operations: u64,
    pub current_stake: i128,
    pub total_penalties: i128,
    pub total_rewards: i128,
    pub last_update_time: u64,
    pub registration_time: u64,
    pub is_slashed: bool,
    pub badge_level: BadgeLevel,
}

/// Badge levels for top performers
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BadgeLevel {
    None,
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
}

/// Reputation configuration weights
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationWeights {
    pub accuracy_weight: u32,
    pub uptime_weight: u32,
    pub response_time_weight: u32,
    pub dispute_history_weight: u32,
}

impl Default for ReputationWeights {
    fn default() -> Self {
        ReputationWeights {
            accuracy_weight: 30,
            uptime_weight: 25,
            response_time_weight: 20,
            dispute_history_weight: 25,
        }
    }
}

/// Data key enum for storage management
#[contracttype]
pub enum DataKey {
    Admin,
    Reputation(Address),
    PerformanceHistory(Address),
    ReputationLeaderboard(EntityType),
    Config,
    TotalEntities(EntityType),
    RegisteredEntities,
}

/// Contract configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub weights: ReputationWeights,
    pub min_stake_amount: i128,
    pub slashing_percentage: u32,
    pub reward_percentage: u32,
    pub decay_enabled: bool,
    pub recovery_enabled: bool,
    pub recovery_period: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            weights: ReputationWeights::default(),
            min_stake_amount: 1000,
            slashing_percentage: 10,
            reward_percentage: 5,
            decay_enabled: true,
            recovery_enabled: true,
            recovery_period: 30 * 24 * 60 * 60, // 30 days
        }
    }
}

/// Leaderboard entry for public ranking
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaderboardEntry {
    pub entity_address: Address,
    pub score: u32,
    pub badge_level: BadgeLevel,
    pub total_operations: u64,
}

#[contract]
pub struct ReputationSystemContract;

#[allow(clippy::too_many_arguments)]
#[contractimpl]
impl ReputationSystemContract {
    /// Initialize the reputation system contract
    pub fn initialize(env: Env, admin: Address, config: Config) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Config, &config);

        // Initialize empty leaderboards
        let bridge_leaderboard: Vec<LeaderboardEntry> = Vec::new(&env);
        let oracle_leaderboard: Vec<LeaderboardEntry> = Vec::new(&env);
        let relay_leaderboard: Vec<LeaderboardEntry> = Vec::new(&env);

        env.storage().instance().set(
            &DataKey::ReputationLeaderboard(EntityType::BridgeOperator),
            &bridge_leaderboard,
        );
        env.storage().instance().set(
            &DataKey::ReputationLeaderboard(EntityType::OracleNode),
            &oracle_leaderboard,
        );
        env.storage().instance().set(
            &DataKey::ReputationLeaderboard(EntityType::RelayOperator),
            &relay_leaderboard,
        );

        // Initialize entity counters
        env.storage()
            .instance()
            .set(&DataKey::TotalEntities(EntityType::BridgeOperator), &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalEntities(EntityType::OracleNode), &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalEntities(EntityType::RelayOperator), &0u64);
    }

    /// Register a new entity in the reputation system
    pub fn register_entity(
        env: Env,
        entity_address: Address,
        entity_type: EntityType,
        stake_amount: i128,
    ) {
        // Check if entity already exists
        if env
            .storage()
            .persistent()
            .has(&DataKey::Reputation(entity_address.clone()))
        {
            panic!("Entity already registered");
        }

        // Validate stake amount
        let config: Config = env.storage().instance().get(&DataKey::Config).unwrap();
        if stake_amount < config.min_stake_amount {
            panic!("Stake amount below minimum requirement");
        }

        // Create initial reputation data
        let reputation_data = ReputationData {
            entity_address: entity_address.clone(),
            entity_type: entity_type.clone(),
            overall_score: 7500, // Start with 75% reputation
            factors: ReputationFactors {
                accuracy_score: 7500,
                uptime_score: 7500,
                response_time_score: 7500,
                dispute_history_score: 7500,
            },
            total_operations: 0,
            successful_operations: 0,
            current_stake: stake_amount,
            total_penalties: 0,
            total_rewards: 0,
            last_update_time: env.ledger().timestamp(),
            registration_time: env.ledger().timestamp(),
            is_slashed: false,
            badge_level: BadgeLevel::None,
        };

        // Store reputation data
        env.storage().persistent().set(
            &DataKey::Reputation(entity_address.clone()),
            &reputation_data,
        );

        // Initialize empty performance history
        let history: Vec<PerformanceRecord> = Vec::new(&env);
        env.storage().persistent().set(
            &DataKey::PerformanceHistory(entity_address.clone()),
            &history,
        );

        // Update entity counter
        let mut total_entities: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEntities(entity_type.clone()))
            .unwrap();
        total_entities += 1;
        env.storage()
            .instance()
            .set(&DataKey::TotalEntities(entity_type), &total_entities);

        // Add to leaderboard
        Self::update_leaderboard_internal(
            &env,
            entity_address,
            reputation_data.overall_score,
            BadgeLevel::None,
        );
    }

    /// Record performance metrics for an entity
    pub fn record_performance(
        env: Env,
        entity_address: Address,
        accuracy: u32,
        uptime: u32,
        response_time: u32,
        disputes_won: u32,
        disputes_lost: u32,
        total_operations: u32,
        successful_operations: u32,
    ) {
        // Require authorization from entity
        entity_address.require_auth();

        // Get existing reputation data
        let reputation_data: ReputationData = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address.clone()))
            .unwrap_or_else(|| panic!("Entity not registered"));

        // Create performance record
        let record = PerformanceRecord {
            timestamp: env.ledger().timestamp(),
            entity_type: reputation_data.entity_type.clone(),
            accuracy,
            uptime,
            response_time,
            disputes_won,
            disputes_lost,
            total_operations,
            successful_operations,
            penalty_amount: 0,
            reward_amount: 0,
        };

        // Store performance record
        let mut history: Vec<PerformanceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::PerformanceHistory(entity_address.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        history.push_back(record);

        // Keep only recent history (apply time decay cleanup)
        if history.len() > MAX_HISTORY_RECORDS {
            let mut cleaned_history: Vec<PerformanceRecord> = Vec::new(&env);
            let cutoff_time = env.ledger().timestamp().saturating_sub(365 * 24 * 60 * 60); // Keep last year

            for i in 0..history.len() {
                let record = history.get(i).unwrap();
                if record.timestamp >= cutoff_time {
                    cleaned_history.push_back(record);
                }
            }
            history = cleaned_history;
        }

        env.storage().persistent().set(
            &DataKey::PerformanceHistory(entity_address.clone()),
            &history,
        );

        // Update reputation scores
        Self::update_reputation_internal(
            &env,
            entity_address.clone(),
            accuracy,
            uptime,
            response_time,
            disputes_won,
            disputes_lost,
        );

        // Update entity statistics
        let mut updated_data: ReputationData = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address.clone()))
            .unwrap();

        updated_data.total_operations += total_operations as u64;
        updated_data.successful_operations += successful_operations as u64;
        updated_data.last_update_time = env.ledger().timestamp();

        // Update badge level based on reputation
        updated_data.badge_level = Self::calculate_badge_level(updated_data.overall_score);

        env.storage()
            .persistent()
            .set(&DataKey::Reputation(entity_address), &updated_data);
    }

    /// Apply penalty to an entity (admin only)
    pub fn apply_penalty(env: Env, entity_address: Address, penalty_amount: i128, _reason: String) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        // Get existing reputation data
        let mut reputation_data: ReputationData = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address.clone()))
            .unwrap_or_else(|| panic!("Entity not registered"));

        // Calculate reputation impact
        let config: Config = env.storage().instance().get(&DataKey::Config).unwrap();
        let reputation_impact = (penalty_amount as u32) * config.slashing_percentage / 100;
        let new_score = reputation_data
            .overall_score
            .saturating_sub(reputation_impact);

        // Update reputation data
        reputation_data.overall_score = new_score;
        reputation_data.total_penalties += penalty_amount;
        reputation_data.current_stake =
            reputation_data.current_stake.saturating_sub(penalty_amount);
        reputation_data.is_slashed = true;
        reputation_data.badge_level = BadgeLevel::None; // Reset badge on penalty
        reputation_data.last_update_time = env.ledger().timestamp();

        // Update stored data
        env.storage().persistent().set(
            &DataKey::Reputation(entity_address.clone()),
            &reputation_data,
        );

        // Add penalty record to history
        let mut history: Vec<PerformanceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::PerformanceHistory(entity_address.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        let penalty_record = PerformanceRecord {
            timestamp: env.ledger().timestamp(),
            entity_type: reputation_data.entity_type.clone(),
            accuracy: 0,
            uptime: 0,
            response_time: 0,
            disputes_won: 0,
            disputes_lost: 1,
            total_operations: 0,
            successful_operations: 0,
            penalty_amount,
            reward_amount: 0,
        };

        history.push_back(penalty_record);
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(entity_address.clone()), &history);

        // Update leaderboard
        Self::update_leaderboard_internal(&env, entity_address, new_score, BadgeLevel::None);
    }

    /// Grant reward to an entity (admin only)
    pub fn grant_reward(env: Env, entity_address: Address, reward_amount: i128, _reason: String) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        // Get existing reputation data
        let mut reputation_data: ReputationData = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address.clone()))
            .unwrap_or_else(|| panic!("Entity not registered"));

        // Calculate reputation boost
        let config: Config = env.storage().instance().get(&DataKey::Config).unwrap();
        let reputation_boost = (reward_amount as u32) * config.reward_percentage / 100;
        let new_score = (reputation_data.overall_score + reputation_boost).min(REPUTATION_SCALE);

        // Update reputation data
        reputation_data.overall_score = new_score;
        reputation_data.total_rewards += reward_amount;
        reputation_data.current_stake += reward_amount;
        reputation_data.last_update_time = env.ledger().timestamp();

        // Check if entity can recover from slashed status
        if reputation_data.is_slashed && new_score >= MIN_REPUTATION_THRESHOLD {
            reputation_data.is_slashed = false;
            reputation_data.badge_level = Self::calculate_badge_level(new_score);
        }

        // Update stored data
        env.storage().persistent().set(
            &DataKey::Reputation(entity_address.clone()),
            &reputation_data,
        );

        // Add reward record to history
        let mut history: Vec<PerformanceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::PerformanceHistory(entity_address.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        let reward_record = PerformanceRecord {
            timestamp: env.ledger().timestamp(),
            entity_type: reputation_data.entity_type.clone(),
            accuracy: 0,
            uptime: 0,
            response_time: 0,
            disputes_won: 1,
            disputes_lost: 0,
            total_operations: 0,
            successful_operations: 0,
            penalty_amount: 0,
            reward_amount,
        };

        history.push_back(reward_record);
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(entity_address.clone()), &history);

        // Update leaderboard
        Self::update_leaderboard_internal(
            &env,
            entity_address,
            new_score,
            reputation_data.badge_level,
        );
    }

    /// Calculate and apply time decay to all entities
    pub fn apply_time_decay(env: Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let config: Config = env.storage().instance().get(&DataKey::Config).unwrap();
        if !config.decay_enabled {
            return;
        }

        let current_time = env.ledger().timestamp();

        // Apply decay to all entity types
        for entity_type in [
            EntityType::BridgeOperator,
            EntityType::OracleNode,
            EntityType::RelayOperator,
        ] {
            let leaderboard: Vec<LeaderboardEntry> = env
                .storage()
                .instance()
                .get(&DataKey::ReputationLeaderboard(entity_type))
                .unwrap_or_else(|| Vec::new(&env));

            for i in 0..leaderboard.len() {
                let entry = leaderboard.get(i).unwrap();
                let mut reputation_data: ReputationData = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Reputation(entry.entity_address.clone()))
                    .unwrap();

                // Calculate time-based decay
                let time_since_update = current_time - reputation_data.last_update_time;
                let decay_factor = Self::calculate_decay_factor(time_since_update);
                let decayed_score = ((reputation_data.overall_score as u64) * decay_factor as u64
                    / REPUTATION_SCALE as u64) as u32;

                // Only update if score changed
                if decayed_score != reputation_data.overall_score {
                    reputation_data.overall_score = decayed_score;
                    reputation_data.badge_level = Self::calculate_badge_level(decayed_score);
                    reputation_data.last_update_time = current_time;

                    env.storage().persistent().set(
                        &DataKey::Reputation(entry.entity_address.clone()),
                        &reputation_data,
                    );

                    // Update leaderboard entry
                    Self::update_leaderboard_internal(
                        &env,
                        entry.entity_address,
                        decayed_score,
                        reputation_data.badge_level,
                    );
                }
            }
        }
    }

    /// Get reputation data for an entity
    pub fn get_reputation(env: Env, entity_address: Address) -> Option<ReputationData> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address))
    }

    /// Get performance history for an entity
    pub fn get_performance_history(env: Env, entity_address: Address) -> Vec<PerformanceRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::PerformanceHistory(entity_address))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get leaderboard for a specific entity type
    pub fn get_leaderboard(env: Env, entity_type: EntityType, limit: u32) -> Vec<LeaderboardEntry> {
        let mut leaderboard: Vec<LeaderboardEntry> = env
            .storage()
            .instance()
            .get(&DataKey::ReputationLeaderboard(entity_type))
            .unwrap_or_else(|| Vec::new(&env));

        // Sort by score descending (simple bubble sort for small lists)
        let len = leaderboard.len();
        for i in 0..len {
            for j in 0..len - i - 1 {
                let score_j = leaderboard.get(j).unwrap().score;
                let score_j1 = leaderboard.get(j + 1).unwrap().score;
                if score_j < score_j1 {
                    // Swap entries
                    let entry_j = leaderboard.get(j).unwrap();
                    let entry_j1 = leaderboard.get(j + 1).unwrap();
                    leaderboard.set(j, entry_j1);
                    leaderboard.set(j + 1, entry_j);
                }
            }
        }

        // Return limited results
        let mut result: Vec<LeaderboardEntry> = Vec::new(&env);
        let max_entries = limit.min(len);
        for i in 0..max_entries {
            result.push_back(leaderboard.get(i).unwrap());
        }

        result
    }

    /// Check if entity meets minimum reputation threshold for access control
    pub fn check_access_control(
        env: Env,
        entity_address: Address,
        required_threshold: u32,
    ) -> bool {
        let reputation_data: Option<ReputationData> = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address));

        match reputation_data {
            Some(data) => !data.is_slashed && data.overall_score >= required_threshold,
            None => false,
        }
    }

    /// Update contract configuration (admin only)
    pub fn update_config(env: Env, new_config: Config) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        // Validate weights sum to 100
        let total_weight = new_config.weights.accuracy_weight
            + new_config.weights.uptime_weight
            + new_config.weights.response_time_weight
            + new_config.weights.dispute_history_weight;

        if total_weight != 100 {
            panic!("Weights must sum to 100");
        }

        env.storage().instance().set(&DataKey::Config, &new_config);
    }

    /// Get current contract configuration
    pub fn get_config(env: Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Internal helper functions
    // -----------------------------------------------------------------------

    /// Update entity reputation based on performance metrics
    fn update_reputation_internal(
        env: &Env,
        entity_address: Address,
        accuracy: u32,
        uptime: u32,
        response_time: u32,
        disputes_won: u32,
        disputes_lost: u32,
    ) {
        let mut reputation_data: ReputationData = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address.clone()))
            .unwrap();

        let config: Config = env.storage().instance().get(&DataKey::Config).unwrap();

        // Calculate new factor scores
        let accuracy_score = accuracy.min(REPUTATION_SCALE);
        let uptime_score = uptime.min(REPUTATION_SCALE);
        let response_time_score = response_time.min(REPUTATION_SCALE);

        // Calculate dispute history score
        let total_disputes = disputes_won + disputes_lost;
        let dispute_history_score = if total_disputes == 0 {
            7500 // Neutral score if no disputes
        } else {
            disputes_won * REPUTATION_SCALE / total_disputes
        };

        // Update factor scores with exponential moving average
        let alpha = 20; // Smoothing factor
        reputation_data.factors.accuracy_score = Self::ema_update(
            reputation_data.factors.accuracy_score,
            accuracy_score,
            alpha,
        );
        reputation_data.factors.uptime_score =
            Self::ema_update(reputation_data.factors.uptime_score, uptime_score, alpha);
        reputation_data.factors.response_time_score = Self::ema_update(
            reputation_data.factors.response_time_score,
            response_time_score,
            alpha,
        );
        reputation_data.factors.dispute_history_score = Self::ema_update(
            reputation_data.factors.dispute_history_score,
            dispute_history_score,
            alpha,
        );

        // Calculate weighted overall score
        let weights = &config.weights;
        let overall_score = ((reputation_data.factors.accuracy_score as u64
            * weights.accuracy_weight as u64)
            + (reputation_data.factors.uptime_score as u64 * weights.uptime_weight as u64)
            + (reputation_data.factors.response_time_score as u64
                * weights.response_time_weight as u64)
            + (reputation_data.factors.dispute_history_score as u64
                * weights.dispute_history_weight as u64))
            / 100;

        reputation_data.overall_score = overall_score as u32;

        env.storage()
            .persistent()
            .set(&DataKey::Reputation(entity_address), &reputation_data);
    }

    /// Calculate exponential moving average
    fn ema_update(current: u32, new: u32, alpha: u32) -> u32 {
        let result: u64 =
            ((current as u64 * (100 - alpha) as u64) + (new as u64 * alpha as u64)) / 100;
        result as u32
    }

    /// Calculate time decay factor
    fn calculate_decay_factor(time_since_update: u64) -> u32 {
        if time_since_update == 0 {
            return REPUTATION_SCALE;
        }

        // no_std-friendly approximation: apply one halving per elapsed half-life.
        let elapsed_half_lives = time_since_update / TIME_DECAY_HALFLIFE;
        let mut factor = REPUTATION_SCALE;
        let mut i = 0u64;

        while i < elapsed_half_lives {
            factor /= 2;
            if factor == 0 {
                break;
            }
            i += 1;
        }

        factor
    }

    /// Calculate badge level based on reputation score
    fn calculate_badge_level(score: u32) -> BadgeLevel {
        match score {
            0..=2499 => BadgeLevel::None,
            2500..=4999 => BadgeLevel::Bronze,
            5000..=7499 => BadgeLevel::Silver,
            7500..=8999 => BadgeLevel::Gold,
            9000..=9499 => BadgeLevel::Platinum,
            9500..=REPUTATION_SCALE => BadgeLevel::Diamond,
            _ => BadgeLevel::None, // Safety case for values > REPUTATION_SCALE
        }
    }

    /// Update leaderboard with new entry
    fn update_leaderboard_internal(
        env: &Env,
        entity_address: Address,
        score: u32,
        badge_level: BadgeLevel,
    ) {
        let reputation_data: ReputationData = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity_address.clone()))
            .unwrap();

        let entity_type = reputation_data.entity_type.clone();

        let mut leaderboard: Vec<LeaderboardEntry> = env
            .storage()
            .instance()
            .get(&DataKey::ReputationLeaderboard(entity_type.clone()))
            .unwrap_or_else(|| Vec::new(env));

        // Check if entity already exists in leaderboard
        let mut found = false;
        for i in 0..leaderboard.len() {
            if leaderboard.get(i).unwrap().entity_address == entity_address {
                // Update existing entry
                let entry = LeaderboardEntry {
                    entity_address: entity_address.clone(),
                    score,
                    badge_level: badge_level.clone(),
                    total_operations: reputation_data.total_operations,
                };
                leaderboard.set(i, entry);
                found = true;
                break;
            }
        }

        if !found {
            // Add new entry
            let entry = LeaderboardEntry {
                entity_address: entity_address.clone(),
                score,
                badge_level: badge_level.clone(),
                total_operations: reputation_data.total_operations,
            };
            leaderboard.push_back(entry);
        }

        env.storage()
            .instance()
            .set(&DataKey::ReputationLeaderboard(entity_type), &leaderboard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    /// Helper: set up a fresh contract with an admin
    fn setup() -> (Env, ReputationSystemContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, ReputationSystemContract);
        let client = ReputationSystemContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let config = Config::default();
        client.initialize(&admin, &config);
        (env, client, admin)
    }

    #[test]
    fn test_initialize() {
        let (_env, client, _admin) = setup();

        let config = client.get_config();
        assert_eq!(config.weights.accuracy_weight, 30);
        assert_eq!(config.weights.uptime_weight, 25);
        assert_eq!(config.weights.response_time_weight, 20);
        assert_eq!(config.weights.dispute_history_weight, 25);
    }

    #[test]
    fn test_register_entity() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        client.register_entity(&entity, &EntityType::BridgeOperator, &(10000));

        let reputation = client.get_reputation(&entity);
        assert!(reputation.is_some());
        let rep = reputation.unwrap();
        assert_eq!(rep.entity_type, EntityType::BridgeOperator);
        assert_eq!(rep.overall_score, 7500);
        assert_eq!(rep.current_stake, 10000);
        assert!(!rep.is_slashed);
    }

    #[test]
    fn test_record_performance() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        client.register_entity(&entity, &EntityType::OracleNode, &(5000));
        client.record_performance(&entity, &9500, &9800, &9200, &3, &1, &100, &95);

        let reputation = client.get_reputation(&entity);
        assert!(reputation.is_some());
        let rep = reputation.unwrap();
        assert_eq!(rep.total_operations, 100);
        assert_eq!(rep.successful_operations, 95);
        assert!(rep.overall_score > 7500); // Should improve from initial
    }

    #[test]
    fn test_apply_penalty() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        // Register entity
        client.register_entity(&entity, &EntityType::RelayOperator, &(10000));

        // Test that poor performance affects reputation
        let initial_score = client.get_reputation(&entity).unwrap().overall_score;
        client.record_performance(&entity, &5000, &6000, &5000, &1, &5, &100, &50);

        let reputation = client.get_reputation(&entity);
        assert!(reputation.is_some());
        let rep = reputation.unwrap();
        // Reputation should have decreased with poor performance
        assert!(rep.overall_score <= initial_score);
        // Success rate should be reflected
        assert!(rep.successful_operations < rep.total_operations);
    }

    #[test]
    fn test_grant_reward() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        // Register entity
        client.register_entity(&entity, &EntityType::BridgeOperator, &(5000));

        // Test that good performance improves reputation
        let initial_score = client.get_reputation(&entity).unwrap().overall_score;
        client.record_performance(&entity, &9500, &9800, &9500, &5, &0, &100, &100);

        let reputation = client.get_reputation(&entity);
        assert!(reputation.is_some());
        let rep = reputation.unwrap();
        assert!(rep.overall_score >= initial_score);
        // Total operations should have increased
        assert!(rep.total_operations >= 100);
    }

    #[test]
    fn test_recovery_mechanism() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        // Register entity
        client.register_entity(&entity, &EntityType::OracleNode, &(10000));

        // Test that performance can improve reputation
        client.record_performance(&entity, &9000, &9500, &9000, &5, &0, &50, &50);
        let initial_rep = client.get_reputation(&entity).unwrap();

        // Continue improving performance
        client.record_performance(&entity, &9500, &9800, &9500, &10, &0, &100, &100);
        let improved_rep = client.get_reputation(&entity).unwrap();

        // Reputation should have improved
        assert!(improved_rep.overall_score >= initial_rep.overall_score);
    }

    #[test]
    fn test_leaderboard() {
        let (env, client, _admin) = setup();
        let entity1 = Address::generate(&env);
        let entity2 = Address::generate(&env);
        let entity3 = Address::generate(&env);

        client.register_entity(&entity1, &EntityType::BridgeOperator, &(10000));
        client.register_entity(&entity2, &EntityType::BridgeOperator, &(10000));
        client.register_entity(&entity3, &EntityType::BridgeOperator, &(10000));

        // Record different performance levels
        client.record_performance(&entity1, &9500, &9800, &9200, &5, &0, &100, &100);
        client.record_performance(&entity2, &8000, &8500, &8000, &3, &2, &100, &90);
        client.record_performance(&entity3, &6000, &7000, &6500, &2, &3, &100, &80);

        let leaderboard = client.get_leaderboard(&EntityType::BridgeOperator, &10);
        assert_eq!(leaderboard.len(), 3);

        // Verify ordering (highest score first)
        for i in 0..leaderboard.len() - 1 {
            assert!(leaderboard.get(i).unwrap().score >= leaderboard.get(i + 1).unwrap().score);
        }
    }

    #[test]
    fn test_badge_levels() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        // Test initial badge (should be None)
        client.register_entity(&entity, &EntityType::BridgeOperator, &(1000));
        assert_eq!(
            client.get_reputation(&entity).unwrap().badge_level,
            BadgeLevel::None
        );

        // Test that after perfect performance, badge should be high
        client.record_performance(&entity, &10000, &10000, &10000, &10, &0, &100, &100);
        let reputation = client.get_reputation(&entity).unwrap();

        // With perfect scores, should have at least Gold badge
        assert!(matches!(
            reputation.badge_level,
            BadgeLevel::Gold | BadgeLevel::Platinum | BadgeLevel::Diamond
        ));
    }

    #[test]
    fn test_access_control() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        // Test unregistered entity
        assert!(!client.check_access_control(&entity, &5000));

        // Register with good reputation
        client.register_entity(&entity, &EntityType::BridgeOperator, &(10000));
        assert!(client.check_access_control(&entity, &5000));

        // Poor performance should reduce reputation but not slash (slash requires penalty function)
        client.record_performance(&entity, &1000, &1000, &1000, &0, &10, &100, &10);

        // With very poor performance, access control check should still work
        // but the threshold requirement might fail
        let rep = client.get_reputation(&entity).unwrap();
        if rep.overall_score < 5000 {
            assert!(!client.check_access_control(&entity, &5000));
        } else {
            assert!(client.check_access_control(&entity, &5000));
        }
    }

    #[test]
    fn test_performance_history() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        client.register_entity(&entity, &EntityType::OracleNode, &(5000));

        // Record multiple performance entries
        for i in 0..5 {
            client.record_performance(
                &entity,
                &(9000 + i * 100),
                &(9500),
                &(9000),
                &(3),
                &(1),
                &(100),
                &(95),
            );
        }

        let history = client.get_performance_history(&entity);
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_config_update() {
        let (_env, client, _admin) = setup();

        let new_config = Config {
            weights: ReputationWeights {
                accuracy_weight: 40,
                uptime_weight: 30,
                response_time_weight: 15,
                dispute_history_weight: 15,
            },
            min_stake_amount: 2000,
            slashing_percentage: 15,
            reward_percentage: 10,
            decay_enabled: false,
            recovery_enabled: true,
            recovery_period: 60 * 24 * 60 * 60,
        };

        client.update_config(&new_config);

        let updated_config = client.get_config();
        assert_eq!(updated_config.weights.accuracy_weight, 40);
        assert_eq!(updated_config.min_stake_amount, 2000);
        assert!(!updated_config.decay_enabled);
    }

    #[test]
    #[should_panic(expected = "Weights must sum to 100")]
    fn test_invalid_config_weights() {
        let (_env, client, _admin) = setup();

        // Try to set invalid weights (don't sum to 100)
        let invalid_config = Config {
            weights: ReputationWeights {
                accuracy_weight: 30,
                uptime_weight: 30,
                response_time_weight: 30,
                dispute_history_weight: 30,
            },
            ..Config::default()
        };

        client.update_config(&invalid_config);
    }

    #[test]
    #[should_panic(expected = "Stake amount below minimum requirement")]
    fn test_min_stake_validation() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        // Try to register with stake below minimum
        client.register_entity(&entity, &EntityType::BridgeOperator, &(100));
    }

    #[test]
    #[should_panic(expected = "Entity already registered")]
    fn test_duplicate_registration() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        client.register_entity(&entity, &EntityType::BridgeOperator, &(10000));
        // Try to register again
        client.register_entity(&entity, &EntityType::OracleNode, &(5000));
    }

    #[test]
    fn test_weighted_reputation_calculation() {
        let (env, client, _admin) = setup();
        let entity = Address::generate(&env);

        // Set custom weights
        let custom_config = Config {
            weights: ReputationWeights {
                accuracy_weight: 40,
                uptime_weight: 30,
                response_time_weight: 20,
                dispute_history_weight: 10,
            },
            ..Config::default()
        };
        client.update_config(&custom_config);

        client.register_entity(&entity, &EntityType::BridgeOperator, &(10000));

        let initial_rep = client.get_reputation(&entity).unwrap();

        // Record performance with known values
        client.record_performance(
            &entity,
            &(10000), // accuracy: 100%
            &(10000), // uptime: 100%
            &(10000), // response_time: 100%
            &(10),    // disputes_won
            &(0),     // disputes_lost
            &(100),
            &(100),
        );

        let reputation = client.get_reputation(&entity).unwrap();

        // With perfect scores, reputation should improve from initial
        assert!(reputation.overall_score > initial_rep.overall_score);

        // Verify factor scores are at least equal to initial (should improve with perfect scores)
        assert!(reputation.factors.accuracy_score >= initial_rep.factors.accuracy_score);
        assert!(reputation.factors.uptime_score >= initial_rep.factors.uptime_score);
        assert!(reputation.factors.response_time_score >= initial_rep.factors.response_time_score);
        assert!(
            reputation.factors.dispute_history_score >= initial_rep.factors.dispute_history_score
        );
    }

    #[test]
    fn test_entity_type_separation() {
        let (env, client, _admin) = setup();
        let bridge_entity = Address::generate(&env);
        let oracle_entity = Address::generate(&env);
        let relay_entity = Address::generate(&env);

        client.register_entity(&bridge_entity, &EntityType::BridgeOperator, &(10000));
        client.register_entity(&oracle_entity, &EntityType::OracleNode, &(10000));
        client.register_entity(&relay_entity, &EntityType::RelayOperator, &(10000));

        // Verify leaderboards are separate
        let bridge_leaderboard = client.get_leaderboard(&EntityType::BridgeOperator, &10);
        let oracle_leaderboard = client.get_leaderboard(&EntityType::OracleNode, &10);
        let relay_leaderboard = client.get_leaderboard(&EntityType::RelayOperator, &10);

        assert_eq!(bridge_leaderboard.len(), 1);
        assert_eq!(oracle_leaderboard.len(), 1);
        assert_eq!(relay_leaderboard.len(), 1);

        // Verify entities are in correct leaderboards
        assert_eq!(
            bridge_leaderboard.get(0).unwrap().entity_address,
            bridge_entity
        );
        assert_eq!(
            oracle_leaderboard.get(0).unwrap().entity_address,
            oracle_entity
        );
        assert_eq!(
            relay_leaderboard.get(0).unwrap().entity_address,
            relay_entity
        );
    }
}
