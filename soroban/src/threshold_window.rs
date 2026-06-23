use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::keys;

pub const MAX_WINDOWS: u32 = 10;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowUnit {
    Seconds,
    Minutes,
    Hours,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowConfig {
    pub window_id: String,
    pub length: u64,
    pub unit: WindowUnit,
    pub threshold_bps: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowEvaluation {
    pub window_id: String,
    pub value: i128,
    pub threshold_bps: u32,
    pub is_breached: bool,
    pub breach_bps: i128,
    pub evaluated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThresholdWindowKey {
    Config(String),
    AllWindows,
}

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    if *caller != admin {
        panic!("only admin can manage threshold windows");
    }
}

pub fn create_window(
    env: &Env,
    caller: &Address,
    window_id: String,
    length: u64,
    unit: WindowUnit,
    threshold_bps: u32,
) {
    require_admin(env, caller);

    if window_id.is_empty() {
        panic!("window_id cannot be empty");
    }
    if length == 0 {
        panic!("window length must be greater than 0");
    }
    if threshold_bps == 0 || threshold_bps > 10_000 {
        panic!("threshold must be between 1 and 10_000 bps");
    }

    let key = ThresholdWindowKey::Config(window_id.clone());
    if env.storage().persistent().has(&key) {
        panic!("window already exists");
    }

    let all_key = ThresholdWindowKey::AllWindows;
    let all: Vec<String> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    if all.len() >= MAX_WINDOWS {
        panic!("maximum number of windows reached");
    }

    let now = env.ledger().timestamp();
    let config = WindowConfig {
        window_id: window_id.clone(),
        length,
        unit: unit.clone(),
        threshold_bps,
        created_at: now,
        updated_at: now,
    };

    env.storage().persistent().set(&key, &config);

    let mut all = all;
    all.push_back(window_id.clone());
    env.storage().persistent().set(&all_key, &all);

    env.events().publish(
        (symbol_short!("win_crt"),),
        (window_id, length, threshold_bps),
    );
}

pub fn update_window(
    env: &Env,
    caller: &Address,
    window_id: String,
    length: u64,
    unit: WindowUnit,
    threshold_bps: u32,
) {
    require_admin(env, caller);

    if length == 0 {
        panic!("window length must be greater than 0");
    }
    if threshold_bps == 0 || threshold_bps > 10_000 {
        panic!("threshold must be between 1 and 10_000 bps");
    }

    let key = ThresholdWindowKey::Config(window_id.clone());
    let mut config: WindowConfig = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("window not found"));

    config.length = length;
    config.unit = unit;
    config.threshold_bps = threshold_bps;
    config.updated_at = env.ledger().timestamp();

    env.storage().persistent().set(&key, &config);

    env.events().publish(
        (symbol_short!("win_upd"),),
        (window_id, length, threshold_bps),
    );
}

pub fn remove_window(env: &Env, caller: &Address, window_id: String) {
    require_admin(env, caller);

    let key = ThresholdWindowKey::Config(window_id.clone());
    if !env.storage().persistent().has(&key) {
        panic!("window not found");
    }

    env.storage().persistent().remove(&key);

    let all_key = ThresholdWindowKey::AllWindows;
    let all: Vec<String> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut updated: Vec<String> = Vec::new(env);
    for w in all.iter() {
        if w != window_id {
            updated.push_back(w);
        }
    }
    env.storage().persistent().set(&all_key, &updated);

    env.events().publish((symbol_short!("win_rem"),), window_id);
}

pub fn get_window(env: &Env, window_id: &String) -> Option<WindowConfig> {
    let key = ThresholdWindowKey::Config(window_id.clone());
    env.storage().persistent().get(&key)
}

pub fn get_all_windows(env: &Env) -> Vec<WindowConfig> {
    let all_key = ThresholdWindowKey::AllWindows;
    let all: Vec<String> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut result: Vec<WindowConfig> = Vec::new(env);
    for wid in all.iter() {
        let key = ThresholdWindowKey::Config(wid.clone());
        if let Some(config) = env.storage().persistent().get::<_, WindowConfig>(&key) {
            result.push_back(config);
        }
    }
    result
}

pub fn evaluate_threshold(
    env: &Env,
    window_id: &String,
    reference_value: i128,
    current_value: i128,
) -> Option<WindowEvaluation> {
    let key = ThresholdWindowKey::Config(window_id.clone());
    let config: WindowConfig = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("window not found"));

    let _window_secs = match config.unit {
        WindowUnit::Seconds => config.length,
        WindowUnit::Minutes => config.length * 60,
        WindowUnit::Hours => config.length * 3600,
    };

    if reference_value == 0 {
        return Some(WindowEvaluation {
            window_id: window_id.clone(),
            value: current_value,
            threshold_bps: config.threshold_bps,
            is_breached: false,
            breach_bps: 0,
            evaluated_at: env.ledger().timestamp(),
        });
    }

    let diff = if current_value > reference_value {
        current_value - reference_value
    } else {
        reference_value - current_value
    };

    let deviation_bps = diff * 10_000 / reference_value;
    let is_breached = deviation_bps > i128::from(config.threshold_bps);

    Some(WindowEvaluation {
        window_id: window_id.clone(),
        value: current_value,
        threshold_bps: config.threshold_bps,
        is_breached,
        breach_bps: deviation_bps,
        evaluated_at: env.ledger().timestamp(),
    })
}

pub fn get_window_seconds(config: &WindowConfig) -> u64 {
    match config.unit {
        WindowUnit::Seconds => config.length,
        WindowUnit::Minutes => config.length * 60,
        WindowUnit::Hours => config.length * 3600,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::{Address, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&keys::ADMIN, &admin);
        env.ledger().set_timestamp(1_000_000);
        (env, admin)
    }

    #[test]
    fn test_create_window() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "price_dev_1h"),
            1,
            WindowUnit::Hours,
            500,
        );
        let window = get_window(&env, &String::from_str(&env, "price_dev_1h"));
        assert!(window.is_some());
        let w = window.unwrap();
        assert_eq!(w.threshold_bps, 500);
    }

    #[test]
    fn test_update_window() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        update_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            2,
            WindowUnit::Hours,
            300,
        );
        let window = get_window(&env, &String::from_str(&env, "win1")).unwrap();
        assert_eq!(window.length, 2);
        assert_eq!(window.threshold_bps, 300);
    }

    #[test]
    fn test_remove_window() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        remove_window(&env, &admin, String::from_str(&env, "win1"));
        let window = get_window(&env, &String::from_str(&env, "win1"));
        assert!(window.is_none());
    }

    #[test]
    fn test_get_all_windows() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win2"),
            30,
            WindowUnit::Minutes,
            300,
        );
        let all = get_all_windows(&env);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_evaluate_threshold_no_breach() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        let eval = evaluate_threshold(&env, &String::from_str(&env, "win1"), 1_000_000, 1_020_000);
        assert!(eval.is_some());
        let e = eval.unwrap();
        assert!(!e.is_breached);
    }

    #[test]
    fn test_evaluate_threshold_breach() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        let eval = evaluate_threshold(&env, &String::from_str(&env, "win1"), 1_000_000, 1_100_000);
        assert!(eval.is_some());
        let e = eval.unwrap();
        assert!(e.is_breached);
    }

    #[test]
    fn test_window_seconds_conversion() {
        let config_hours = WindowConfig {
            window_id: String::from_str(&Env::default(), "h"),
            length: 2,
            unit: WindowUnit::Hours,
            threshold_bps: 500,
            created_at: 0,
            updated_at: 0,
        };
        assert_eq!(get_window_seconds(&config_hours), 7200);
    }

    #[test]
    fn test_create_duplicate_window_panics() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
    }

    #[test]
    #[should_panic(expected = "window already exists")]
    fn test_create_duplicate_window_panics_impl() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
    }

    #[test]
    fn test_evaluate_with_zero_reference() {
        let (env, admin) = setup();
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        let eval = evaluate_threshold(&env, &String::from_str(&env, "win1"), 0, 100);
        assert!(eval.is_some());
        let e = eval.unwrap();
        assert!(!e.is_breached);
    }
}
