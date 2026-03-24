#![allow(unused)]

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, Map, String, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AlertType {
    PriceDeviation,
    SupplyMismatch,
    BridgeDowntime,
    HealthScoreDrop,
    VolumeAnomaly,
    ReserveRatioBreach,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompareOp {
    GreaterThan,
    LessThan,
    Equal,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConditionOp {
    And,
    Or,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AlertPriority {
    Critical,
    High,
    Medium,
    Low,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertCondition {
    pub metric: String,
    pub alert_type: AlertType,
    pub compare_op: CompareOp,
    pub threshold: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertRule {
    pub rule_id: u64,
    pub owner: Address,
    pub name: String,
    pub asset_code: String,
    pub conditions: Vec<AlertCondition>,
    pub condition_op: ConditionOp,
    pub priority: AlertPriority,
    pub cooldown_seconds: u64,
    pub is_active: bool,
    pub created_at: u64,
    pub last_triggered: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertEvent {
    pub event_id: u64,
    pub rule_id: u64,
    pub asset_code: String,
    pub alert_type: AlertType,
    pub triggered_value: i128,
    pub threshold: i128,
    pub priority: AlertPriority,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricValue {
    pub metric: String,
    pub value: i128,
}

#[contracttype]
pub enum DataKey {
    Admin,
    AlertRule(u64),
    UserRules(Address),
    AssetAlerts(String),
    RuleCount,
    AlertCount,
}

const MAX_EVENTS_PER_ASSET: u32 = 100;

#[contract]
pub struct AlertSystemContract;

#[contractimpl]
impl AlertSystemContract {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::RuleCount, &0u64);
        env.storage().instance().set(&DataKey::AlertCount, &0u64);
    }

    pub fn register_rule(
        env: Env,
        owner: Address,
        name: String,
        asset_code: String,
        conditions: Vec<AlertCondition>,
        condition_op: ConditionOp,
        priority: AlertPriority,
        cooldown_seconds: u64,
    ) -> u64 {
        owner.require_auth();

        let rule_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RuleCount)
            .unwrap_or(0);
        let rule_id = rule_count + 1;

        let rule = AlertRule {
            rule_id,
            owner: owner.clone(),
            name,
            asset_code,
            conditions,
            condition_op,
            priority,
            cooldown_seconds,
            is_active: true,
            created_at: env.ledger().timestamp(),
            last_triggered: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::AlertRule(rule_id), &rule);

        let mut user_rules: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserRules(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        user_rules.push_back(rule_id);
        env.storage()
            .persistent()
            .set(&DataKey::UserRules(owner), &user_rules);

        env.storage()
            .instance()
            .set(&DataKey::RuleCount, &rule_id);

        rule_id
    }

    pub fn update_rule(
        env: Env,
        rule_id: u64,
        name: String,
        conditions: Vec<AlertCondition>,
        condition_op: ConditionOp,
        priority: AlertPriority,
        cooldown_seconds: u64,
    ) {
        let mut rule: AlertRule = env
            .storage()
            .persistent()
            .get(&DataKey::AlertRule(rule_id))
            .unwrap();
        rule.owner.require_auth();

        rule.name = name;
        rule.conditions = conditions;
        rule.condition_op = condition_op;
        rule.priority = priority;
        rule.cooldown_seconds = cooldown_seconds;

        env.storage()
            .persistent()
            .set(&DataKey::AlertRule(rule_id), &rule);
    }

    pub fn set_rule_active(env: Env, rule_id: u64, is_active: bool) {
        let mut rule: AlertRule = env
            .storage()
            .persistent()
            .get(&DataKey::AlertRule(rule_id))
            .unwrap();

        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let caller_is_admin = rule.owner == admin;
        if !caller_is_admin {
            rule.owner.require_auth();
        } else {
            admin.require_auth();
        }

        rule.is_active = is_active;
        env.storage()
            .persistent()
            .set(&DataKey::AlertRule(rule_id), &rule);
    }

    pub fn evaluate_asset(
        env: Env,
        asset_code: String,
        metrics: Vec<MetricValue>,
    ) -> Vec<AlertEvent> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let rule_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RuleCount)
            .unwrap_or(0);

        let mut triggered: Vec<AlertEvent> = Vec::new(&env);
        let now = env.ledger().timestamp();

        let mut alert_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AlertCount)
            .unwrap_or(0);

        for i in 1u64..=rule_count {
            let rule_opt: Option<AlertRule> =
                env.storage().persistent().get(&DataKey::AlertRule(i));

            let rule = match rule_opt {
                Some(r) => r,
                None => continue,
            };

            if !rule.is_active {
                continue;
            }

            if rule.asset_code != asset_code {
                continue;
            }

            if rule.cooldown_seconds > 0
                && rule.last_triggered > 0
                && now < rule.last_triggered + rule.cooldown_seconds
            {
                continue;
            }

            let (fires, triggered_value, threshold, alert_type) =
                Self::evaluate_conditions(&env, &rule, &metrics);

            if fires {
                alert_count += 1;
                let event = AlertEvent {
                    event_id: alert_count,
                    rule_id: rule.rule_id,
                    asset_code: asset_code.clone(),
                    alert_type,
                    triggered_value,
                    threshold,
                    priority: rule.priority.clone(),
                    timestamp: now,
                };

                let mut history: Vec<AlertEvent> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AssetAlerts(asset_code.clone()))
                    .unwrap_or_else(|| Vec::new(&env));

                if history.len() >= MAX_EVENTS_PER_ASSET {
                    history.pop_front();
                }
                history.push_back(event.clone());
                env.storage()
                    .persistent()
                    .set(&DataKey::AssetAlerts(asset_code.clone()), &history);

                let mut updated_rule: AlertRule = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AlertRule(rule.rule_id))
                    .unwrap();
                updated_rule.last_triggered = now;
                env.storage()
                    .persistent()
                    .set(&DataKey::AlertRule(rule.rule_id), &updated_rule);

                triggered.push_back(event);
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::AlertCount, &alert_count);

        triggered
    }

    pub fn batch_evaluate(
        env: Env,
        asset_metrics: Vec<(String, Vec<MetricValue>)>,
    ) -> Vec<AlertEvent> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let mut all_events: Vec<AlertEvent> = Vec::new(&env);

        for i in 0..asset_metrics.len() {
            let pair = asset_metrics.get(i).unwrap();
            let asset_code = pair.0;
            let metrics = pair.1;

            let events = Self::evaluate_asset_internal(&env, asset_code, metrics);
            for j in 0..events.len() {
                all_events.push_back(events.get(j).unwrap());
            }
        }

        all_events
    }

    pub fn get_rule(env: Env, rule_id: u64) -> Option<AlertRule> {
        env.storage().persistent().get(&DataKey::AlertRule(rule_id))
    }

    pub fn get_user_rules(env: Env, owner: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::UserRules(owner))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_asset_alerts(env: Env, asset_code: String) -> Vec<AlertEvent> {
        env.storage()
            .persistent()
            .get(&DataKey::AssetAlerts(asset_code))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_rule_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::RuleCount)
            .unwrap_or(0)
    }

    pub fn get_alert_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::AlertCount)
            .unwrap_or(0)
    }

    fn evaluate_conditions(
        env: &Env,
        rule: &AlertRule,
        metrics: &Vec<MetricValue>,
    ) -> (bool, i128, i128, AlertType) {
        let conditions = &rule.conditions;
        let len = conditions.len();

        if len == 0 {
            return (false, 0, 0, AlertType::PriceDeviation);
        }

        let mut fires = match rule.condition_op {
            ConditionOp::And => true,
            ConditionOp::Or => false,
        };

        let mut first_triggered_value: i128 = 0;
        let mut first_threshold: i128 = 0;
        let mut first_alert_type = AlertType::PriceDeviation;
        let mut any_triggered = false;

        for i in 0..len {
            let cond = conditions.get(i).unwrap();
            let metric_value = Self::find_metric(env, &cond.metric, metrics);

            let result = match cond.compare_op {
                CompareOp::GreaterThan => metric_value > cond.threshold,
                CompareOp::LessThan => metric_value < cond.threshold,
                CompareOp::Equal => metric_value == cond.threshold,
            };

            if result && !any_triggered {
                first_triggered_value = metric_value;
                first_threshold = cond.threshold;
                first_alert_type = cond.alert_type.clone();
                any_triggered = true;
            }

            fires = match rule.condition_op {
                ConditionOp::And => fires && result,
                ConditionOp::Or => fires || result,
            };
        }

        (fires, first_triggered_value, first_threshold, first_alert_type)
    }

    fn find_metric(_env: &Env, metric: &String, metrics: &Vec<MetricValue>) -> i128 {
        for i in 0..metrics.len() {
            let mv = metrics.get(i).unwrap();
            if &mv.metric == metric {
                return mv.value;
            }
        }
        0
    }

    fn evaluate_asset_internal(
        env: &Env,
        asset_code: String,
        metrics: Vec<MetricValue>,
    ) -> Vec<AlertEvent> {
        let rule_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RuleCount)
            .unwrap_or(0);

        let mut triggered: Vec<AlertEvent> = Vec::new(env);
        let now = env.ledger().timestamp();

        let mut alert_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AlertCount)
            .unwrap_or(0);

        for i in 1u64..=rule_count {
            let rule_opt: Option<AlertRule> =
                env.storage().persistent().get(&DataKey::AlertRule(i));

            let rule = match rule_opt {
                Some(r) => r,
                None => continue,
            };

            if !rule.is_active || rule.asset_code != asset_code {
                continue;
            }

            if rule.cooldown_seconds > 0
                && rule.last_triggered > 0
                && now < rule.last_triggered + rule.cooldown_seconds
            {
                continue;
            }

            let (fires, triggered_value, threshold, alert_type) =
                Self::evaluate_conditions(env, &rule, &metrics);

            if fires {
                alert_count += 1;
                let event = AlertEvent {
                    event_id: alert_count,
                    rule_id: rule.rule_id,
                    asset_code: asset_code.clone(),
                    alert_type,
                    triggered_value,
                    threshold,
                    priority: rule.priority.clone(),
                    timestamp: now,
                };

                let mut history: Vec<AlertEvent> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AssetAlerts(asset_code.clone()))
                    .unwrap_or_else(|| Vec::new(env));

                if history.len() >= MAX_EVENTS_PER_ASSET {
                    history.pop_front();
                }
                history.push_back(event.clone());
                env.storage()
                    .persistent()
                    .set(&DataKey::AssetAlerts(asset_code.clone()), &history);

                let mut updated_rule: AlertRule = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AlertRule(rule.rule_id))
                    .unwrap();
                updated_rule.last_triggered = now;
                env.storage()
                    .persistent()
                    .set(&DataKey::AlertRule(rule.rule_id), &updated_rule);

                triggered.push_back(event);
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::AlertCount, &alert_count);

        triggered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    fn setup() -> (Env, soroban_sdk::Address, soroban_sdk::Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AlertSystemContract);
        let client = AlertSystemContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, contract_id, admin)
    }

    #[test]
    fn test_initialize() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);
        assert_eq!(client.get_rule_count(), 0);
        assert_eq!(client.get_alert_count(), 0);
    }

    #[test]
    fn test_register_rule() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");
        let metric = String::from_str(&env, "price_deviation_bps");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: metric.clone(),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 200,
            }],
        );

        let rule_id = client.register_rule(
            &owner,
            &String::from_str(&env, "Price Alert"),
            &asset,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::High,
            &3600u64,
        );

        assert_eq!(rule_id, 1);
        assert_eq!(client.get_rule_count(), 1);

        let rule = client.get_rule(&rule_id).unwrap();
        assert_eq!(rule.rule_id, 1);
        assert!(rule.is_active);
        assert_eq!(rule.cooldown_seconds, 3600);
    }

    #[test]
    fn test_register_multiple_rules() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        for i in 0u64..3 {
            let conditions = Vec::from_array(
                &env,
                [AlertCondition {
                    metric: String::from_str(&env, "health_score"),
                    alert_type: AlertType::HealthScoreDrop,
                    compare_op: CompareOp::LessThan,
                    threshold: (50 - i as i128),
                }],
            );
            client.register_rule(
                &owner,
                &String::from_str(&env, "Rule"),
                &asset,
                &conditions,
                &ConditionOp::And,
                &AlertPriority::Medium,
                &0u64,
            );
        }

        assert_eq!(client.get_rule_count(), 3);
        let user_rules = client.get_user_rules(&owner);
        assert_eq!(user_rules.len(), 3);
    }

    #[test]
    fn test_evaluate_triggers_alert() {
        let (env, contract_id, admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "price_deviation_bps"),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 200,
            }],
        );

        client.register_rule(
            &owner,
            &String::from_str(&env, "Price Alert"),
            &asset,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::High,
            &0u64,
        );

        let metrics = Vec::from_array(
            &env,
            [MetricValue {
                metric: String::from_str(&env, "price_deviation_bps"),
                value: 350,
            }],
        );

        let events = client.evaluate_asset(&asset, &metrics);
        assert_eq!(events.len(), 1);
        assert_eq!(events.get(0).unwrap().triggered_value, 350);
        assert_eq!(client.get_alert_count(), 1);
    }

    #[test]
    fn test_evaluate_no_trigger_below_threshold() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "price_deviation_bps"),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 200,
            }],
        );

        client.register_rule(
            &owner,
            &String::from_str(&env, "Price Alert"),
            &asset,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::High,
            &0u64,
        );

        let metrics = Vec::from_array(
            &env,
            [MetricValue {
                metric: String::from_str(&env, "price_deviation_bps"),
                value: 100,
            }],
        );

        let events = client.evaluate_asset(&asset, &metrics);
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_cooldown_prevents_retrigger() {
        let (env, contract_id, _admin) = setup();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "price_deviation_bps"),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 100,
            }],
        );

        client.register_rule(
            &owner,
            &String::from_str(&env, "Price Alert"),
            &asset,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::High,
            &7200u64,
        );

        let metrics = Vec::from_array(
            &env,
            [MetricValue {
                metric: String::from_str(&env, "price_deviation_bps"),
                value: 300,
            }],
        );

        let first = client.evaluate_asset(&asset, &metrics);
        assert_eq!(first.len(), 1);

        let second = client.evaluate_asset(&asset, &metrics);
        assert_eq!(second.len(), 0, "cooldown should suppress re-trigger");
    }

    #[test]
    fn test_and_condition_both_must_fire() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [
                AlertCondition {
                    metric: String::from_str(&env, "price_deviation_bps"),
                    alert_type: AlertType::PriceDeviation,
                    compare_op: CompareOp::GreaterThan,
                    threshold: 200,
                },
                AlertCondition {
                    metric: String::from_str(&env, "health_score"),
                    alert_type: AlertType::HealthScoreDrop,
                    compare_op: CompareOp::LessThan,
                    threshold: 50,
                },
            ],
        );

        client.register_rule(
            &owner,
            &String::from_str(&env, "AND Rule"),
            &asset,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::Critical,
            &0u64,
        );

        // Only first condition fires
        let metrics_partial = Vec::from_array(
            &env,
            [
                MetricValue {
                    metric: String::from_str(&env, "price_deviation_bps"),
                    value: 300,
                },
                MetricValue {
                    metric: String::from_str(&env, "health_score"),
                    value: 70,
                },
            ],
        );
        let events = client.evaluate_asset(&asset, &metrics_partial);
        assert_eq!(events.len(), 0, "AND: only one condition fires, should not trigger");

        // Both conditions fire
        let metrics_both = Vec::from_array(
            &env,
            [
                MetricValue {
                    metric: String::from_str(&env, "price_deviation_bps"),
                    value: 300,
                },
                MetricValue {
                    metric: String::from_str(&env, "health_score"),
                    value: 30,
                },
            ],
        );
        let events2 = client.evaluate_asset(&asset, &metrics_both);
        assert_eq!(events2.len(), 1, "AND: both conditions fire, should trigger");
    }

    #[test]
    fn test_or_condition_either_fires() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [
                AlertCondition {
                    metric: String::from_str(&env, "price_deviation_bps"),
                    alert_type: AlertType::PriceDeviation,
                    compare_op: CompareOp::GreaterThan,
                    threshold: 200,
                },
                AlertCondition {
                    metric: String::from_str(&env, "health_score"),
                    alert_type: AlertType::HealthScoreDrop,
                    compare_op: CompareOp::LessThan,
                    threshold: 50,
                },
            ],
        );

        client.register_rule(
            &owner,
            &String::from_str(&env, "OR Rule"),
            &asset,
            &conditions,
            &ConditionOp::Or,
            &AlertPriority::Medium,
            &0u64,
        );

        // Only first condition fires — OR should trigger
        let metrics = Vec::from_array(
            &env,
            [
                MetricValue {
                    metric: String::from_str(&env, "price_deviation_bps"),
                    value: 300,
                },
                MetricValue {
                    metric: String::from_str(&env, "health_score"),
                    value: 70,
                },
            ],
        );
        let events = client.evaluate_asset(&asset, &metrics);
        assert_eq!(events.len(), 1, "OR: one condition fires, should trigger");
    }

    #[test]
    fn test_deactivate_rule() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "price_deviation_bps"),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 100,
            }],
        );

        let rule_id = client.register_rule(
            &owner,
            &String::from_str(&env, "Alert"),
            &asset,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::Low,
            &0u64,
        );

        client.set_rule_active(&rule_id, &false);

        let metrics = Vec::from_array(
            &env,
            [MetricValue {
                metric: String::from_str(&env, "price_deviation_bps"),
                value: 500,
            }],
        );

        let events = client.evaluate_asset(&asset, &metrics);
        assert_eq!(events.len(), 0, "Deactivated rule should not fire");
    }

    #[test]
    fn test_alert_history_per_asset() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "health_score"),
                alert_type: AlertType::HealthScoreDrop,
                compare_op: CompareOp::LessThan,
                threshold: 60,
            }],
        );

        client.register_rule(
            &owner,
            &String::from_str(&env, "Health Alert"),
            &asset,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::High,
            &0u64,
        );

        let metrics = Vec::from_array(
            &env,
            [MetricValue {
                metric: String::from_str(&env, "health_score"),
                value: 40,
            }],
        );

        client.evaluate_asset(&asset, &metrics);

        let history = client.get_asset_alerts(&asset);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().triggered_value, 40);
    }

    #[test]
    fn test_rule_not_triggered_for_different_asset() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let usdc = String::from_str(&env, "USDC");
        let eurc = String::from_str(&env, "EURC");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "price_deviation_bps"),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 100,
            }],
        );

        client.register_rule(
            &owner,
            &String::from_str(&env, "USDC Alert"),
            &usdc,
            &conditions,
            &ConditionOp::And,
            &AlertPriority::High,
            &0u64,
        );

        let metrics = Vec::from_array(
            &env,
            [MetricValue {
                metric: String::from_str(&env, "price_deviation_bps"),
                value: 500,
            }],
        );

        let events = client.evaluate_asset(&eurc, &metrics);
        assert_eq!(events.len(), 0, "Rule for USDC should not trigger for EURC");
    }

    #[test]
    fn test_update_rule() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "price_deviation_bps"),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 200,
            }],
        );

        let rule_id = client.register_rule(
            &owner,
            &String::from_str(&env, "Old Name"),
            &asset,
            &conditions.clone(),
            &ConditionOp::And,
            &AlertPriority::Low,
            &0u64,
        );

        let new_conditions = Vec::from_array(
            &env,
            [AlertCondition {
                metric: String::from_str(&env, "price_deviation_bps"),
                alert_type: AlertType::PriceDeviation,
                compare_op: CompareOp::GreaterThan,
                threshold: 500,
            }],
        );

        client.update_rule(
            &rule_id,
            &String::from_str(&env, "New Name"),
            &new_conditions,
            &ConditionOp::And,
            &AlertPriority::Critical,
            &3600u64,
        );

        let updated = client.get_rule(&rule_id).unwrap();
        assert_eq!(updated.priority, AlertPriority::Critical);
        assert_eq!(updated.cooldown_seconds, 3600);
    }

    #[test]
    fn test_all_alert_types() {
        let (env, contract_id, _admin) = setup();
        let client = AlertSystemContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");

        let alert_types = [
            AlertType::PriceDeviation,
            AlertType::SupplyMismatch,
            AlertType::BridgeDowntime,
            AlertType::HealthScoreDrop,
            AlertType::VolumeAnomaly,
            AlertType::ReserveRatioBreach,
        ];

        let metrics_names = [
            "price_deviation_bps",
            "supply_mismatch_bps",
            "bridge_uptime_pct",
            "health_score",
            "volume_zscore",
            "reserve_ratio_bps",
        ];

        for (i, (alert_type, metric_name)) in
            alert_types.iter().zip(metrics_names.iter()).enumerate()
        {
            let conditions = Vec::from_array(
                &env,
                [AlertCondition {
                    metric: String::from_str(&env, metric_name),
                    alert_type: alert_type.clone(),
                    compare_op: CompareOp::GreaterThan,
                    threshold: 100,
                }],
            );

            client.register_rule(
                &owner,
                &String::from_str(&env, "Alert"),
                &asset,
                &conditions,
                &ConditionOp::And,
                &AlertPriority::High,
                &0u64,
            );
        }

        assert_eq!(client.get_rule_count(), 6);
    }
}
