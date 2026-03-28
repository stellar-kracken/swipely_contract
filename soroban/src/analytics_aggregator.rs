use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Vec,
};

const BUFFER_SIZE: u64 = 168; // 1 week of hourly buckets

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub enum BucketType {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

#[contracttype]
#[derive(Clone)]
pub struct MetricDataPoint {
    pub bucket_start: u64,
    pub value: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct CustomMetric {
    pub name: String,
    pub formula: String,
}

#[contracttype]
#[derive(Clone)]
pub struct DashboardSummary {
    pub tvl: i128,
    pub volume: i128,
    pub user_count: i128,
    pub tx_count: i128,
}

#[contracttype]
pub enum DataKey {
    Admin,
    RegisteredMetrics,
    CustomMetrics,
    MetricBucket(String, BucketType, u64),
    MetricBucketTimestamp(String, BucketType, u64),
}

#[contract]
pub struct AnalyticsAggregatorContract;

#[contractimpl]
impl AnalyticsAggregatorContract {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        assert!(
            !env.storage().instance().has(&DataKey::Admin),
            "already initialized"
        );
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::RegisteredMetrics, &Vec::<String>::new(&env));
        let custom: Map<String, String> = Map::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::CustomMetrics, &custom);
    }

    fn get_admin(env: &Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin = Self::get_admin(env);
        assert!(caller == &admin, "unauthorized");
    }

    fn bucket_length(bucket: &BucketType) -> u64 {
        match bucket {
            BucketType::Hourly => 3600,
            BucketType::Daily => 86400,
            BucketType::Weekly => 604800,
            BucketType::Monthly => 2592000,
        }
    }

    fn round_bucket_start(timestamp: u64, bucket: &BucketType) -> u64 {
        let size = Self::bucket_length(bucket);
        timestamp - (timestamp % size)
    }

    fn slot_index(bucket_start: u64, bucket: &BucketType) -> u64 {
        (bucket_start / Self::bucket_length(bucket)) % BUFFER_SIZE
    }

    fn store_bucket_value(
        env: &Env,
        metric: &String,
        bucket: &BucketType,
        bucket_start: u64,
        value: i128,
    ) {
        let slot = Self::slot_index(bucket_start, bucket);
        let ts_key = DataKey::MetricBucketTimestamp(metric.clone(), bucket.clone(), slot);
        let value_key = DataKey::MetricBucket(metric.clone(), bucket.clone(), slot);

        let existing_ts: u64 = env.storage().instance().get(&ts_key).unwrap_or(0_u64);

        let next_value = if existing_ts == bucket_start {
            let existing_value: i128 = env.storage().instance().get(&value_key).unwrap_or(0_i128);
            existing_value + value
        } else {
            value
        };

        env.storage().instance().set(&ts_key, &bucket_start);
        env.storage().instance().set(&value_key, &next_value);
    }

    fn load_bucket_value(
        env: &Env,
        metric: &String,
        bucket: &BucketType,
        bucket_start: u64,
    ) -> i128 {
        let slot = Self::slot_index(bucket_start, bucket);
        let ts_key = DataKey::MetricBucketTimestamp(metric.clone(), bucket.clone(), slot);
        let value_key = DataKey::MetricBucket(metric.clone(), bucket.clone(), slot);

        let stored_ts: u64 = env.storage().instance().get(&ts_key).unwrap_or(0_u64);

        if stored_ts != bucket_start {
            0
        } else {
            env.storage().instance().get(&value_key).unwrap_or(0_i128)
        }
    }

    fn register_metric_if_missing(env: &Env, metric: &String) {
        let mut metrics: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::RegisteredMetrics)
            .unwrap_or(Vec::new(env));

        let mut already = false;
        for m in metrics.iter() {
            if m == *metric {
                already = true;
                break;
            }
        }

        if !already {
            metrics.push_back(metric.clone());
            env.storage()
                .instance()
                .set(&DataKey::RegisteredMetrics, &metrics);
        }
    }

    pub fn record_metric(env: Env, caller: Address, metric: String, value: i128, timestamp: u64) {
        Self::assert_admin(&env, &caller);
        assert!(value >= 0, "value must be non-negative");

        Self::register_metric_if_missing(&env, &metric);

        for bucket in [
            BucketType::Hourly,
            BucketType::Daily,
            BucketType::Weekly,
            BucketType::Monthly,
        ]
        .iter()
        {
            let bucket_start = Self::round_bucket_start(timestamp, bucket);
            // store_bucket_value handles accumulation internally; just pass the raw value
            Self::store_bucket_value(&env, &metric, bucket, bucket_start, value);
        }

        env.events().publish(
            (symbol_short!("am_rcd"),),
            (metric.clone(), value, timestamp),
        );
    }

    pub fn get_metric_history(
        env: Env,
        metric: String,
        bucket: BucketType,
        limit: u32,
    ) -> Vec<MetricDataPoint> {
        assert!(limit > 0 && limit <= 168, "limit must be 1..168");

        let now = env.ledger().timestamp();
        let current_start = Self::round_bucket_start(now, &bucket);

        let mut history = Vec::new(&env);
        let mut current = current_start;
        for _ in 0..limit {
            let value = Self::load_bucket_value(&env, &metric, &bucket, current);
            let point = MetricDataPoint {
                bucket_start: current,
                value,
            };
            history.push_back(point);
            current = current.saturating_sub(Self::bucket_length(&bucket));
        }

        history
    }

    pub fn set_custom_metric(env: Env, caller: Address, name: String, formula: String) {
        Self::assert_admin(&env, &caller);
        let mut custom: Map<String, String> = env
            .storage()
            .instance()
            .get(&DataKey::CustomMetrics)
            .unwrap_or(Map::new(&env));

        custom.set(name.clone(), formula.clone());
        env.storage()
            .instance()
            .set(&DataKey::CustomMetrics, &custom);

        env.events()
            .publish((symbol_short!("am_cst"),), (name, formula));
    }

    pub fn compute_custom_metric(env: Env, name: String) -> i128 {
        let custom: Map<String, String> = env
            .storage()
            .instance()
            .get(&DataKey::CustomMetrics)
            .unwrap_or(Map::new(&env));

        let formula = custom
            .get(name.clone())
            .unwrap_or(String::from_str(&env, ""));

        if formula == String::from_str(&env, "tvl_per_tx") {
            let tvl = Self::load_bucket_value(
                &env,
                &String::from_str(&env, "tvl"),
                &BucketType::Hourly,
                Self::round_bucket_start(env.ledger().timestamp(), &BucketType::Hourly),
            );
            let tx = Self::load_bucket_value(
                &env,
                &String::from_str(&env, "tx_count"),
                &BucketType::Hourly,
                Self::round_bucket_start(env.ledger().timestamp(), &BucketType::Hourly),
            );
            if tx == 0 {
                return 0;
            }
            return tvl / tx;
        }

        if formula == String::from_str(&env, "avg_user_volume") {
            let volume = Self::load_bucket_value(
                &env,
                &String::from_str(&env, "volume"),
                &BucketType::Hourly,
                Self::round_bucket_start(env.ledger().timestamp(), &BucketType::Hourly),
            );
            let users = Self::load_bucket_value(
                &env,
                &String::from_str(&env, "user_count"),
                &BucketType::Hourly,
                Self::round_bucket_start(env.ledger().timestamp(), &BucketType::Hourly),
            );
            if users == 0 {
                return 0;
            }
            return volume / users;
        }

        panic!("unsupported custom metric formula");
    }

    pub fn get_dashboard_summary(env: Env) -> DashboardSummary {
        let now = env.ledger().timestamp();
        let bucket = BucketType::Hourly;
        let bucket_start = Self::round_bucket_start(now, &bucket);

        DashboardSummary {
            tvl: Self::load_bucket_value(
                &env,
                &String::from_str(&env, "tvl"),
                &bucket,
                bucket_start,
            ),
            volume: Self::load_bucket_value(
                &env,
                &String::from_str(&env, "volume"),
                &bucket,
                bucket_start,
            ),
            user_count: Self::load_bucket_value(
                &env,
                &String::from_str(&env, "user_count"),
                &bucket,
                bucket_start,
            ),
            tx_count: Self::load_bucket_value(
                &env,
                &String::from_str(&env, "tx_count"),
                &bucket,
                bucket_start,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Env,
    };

    #[test]
    fn test_analytics_register_metric_and_history() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AnalyticsAggregatorContract);
        let client = AnalyticsAggregatorContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin);

        let metric = String::from_str(&env, "tvl");
        // Use an exact hour boundary so both recordings fall in the same bucket
        let now = 3600u64;

        client.record_metric(&admin, &metric, &1000_i128, &now);
        client.record_metric(&admin, &metric, &500_i128, &(now + 1800));

        // Stay within the same hourly bucket (bucket starts at 3600, ends at 7199)
        env.ledger().set_timestamp(now + 3599);
        let history = client.get_metric_history(&metric, &BucketType::Hourly, &3);

        assert_eq!(history.len(), 3);
        assert_eq!(history.get(0).unwrap().value, 1500);
    }

    #[test]
    fn test_dashboard_summary_and_custom_metrics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AnalyticsAggregatorContract);
        let client = AnalyticsAggregatorContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin);

        let tvl = String::from_str(&env, "tvl");
        let volume = String::from_str(&env, "volume");
        let users = String::from_str(&env, "user_count");
        let tx = String::from_str(&env, "tx_count");

        let now = 1_000_000u64;
        client.record_metric(&admin, &tvl, &2000_i128, &now);
        client.record_metric(&admin, &volume, &300_i128, &now);
        client.record_metric(&admin, &users, &10_i128, &now);
        client.record_metric(&admin, &tx, &50_i128, &now);

        env.ledger().set_timestamp(now);
        client.set_custom_metric(
            &admin,
            &String::from_str(&env, "tvl_per_tx"),
            &String::from_str(&env, "tvl_per_tx"),
        );
        let computed = client.compute_custom_metric(&String::from_str(&env, "tvl_per_tx"));
        assert_eq!(computed, 2000 / 50);

        let dashboard = client.get_dashboard_summary();
        assert_eq!(dashboard.tvl, 2000);
        assert_eq!(dashboard.tx_count, 50);
    }
}
