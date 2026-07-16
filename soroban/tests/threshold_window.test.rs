#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, Env, String,
};
use swipely_contracts::threshold_window::{
    create_window, evaluate_threshold, get_all_windows, get_window, get_window_seconds,
    remove_window, update_window, WindowConfig, WindowUnit,
};

// Minimal test contract — needed so env.as_contract() can provide a storage context.
// Each env.as_contract() call is one auth frame; functions calling require_auth()
// must each have their own as_contract block to avoid Error(Auth, ExistingValue).
#[contract]
struct TestContract;
#[contractimpl]
impl TestContract {}

// "admin" mirrors the private keys::ADMIN constant value ("admin")
fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, TestContract);
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&"admin", &admin);
    });
    env.ledger().set_timestamp(1_000_000);
    (env, admin, contract_id)
}

#[test]
fn test_create_window_stores_all_fields() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "price_dev_1h"),
            1,
            WindowUnit::Hours,
            500,
        );
        let w = get_window(&env, &String::from_str(&env, "price_dev_1h")).unwrap();
        assert_eq!(w.length, 1);
        assert_eq!(w.threshold_bps, 500);
        assert_eq!(w.created_at, 1_000_000);
        assert_eq!(w.updated_at, 1_000_000);
    });
}

#[test]
fn test_create_window_appears_in_all_list() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            30,
            WindowUnit::Minutes,
            300,
        );
        assert_eq!(get_all_windows(&env).len(), 1);
    });
}

#[test]
fn test_update_window() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
    });

    env.ledger().with_mut(|li| li.timestamp += 100);

    env.as_contract(&contract_id, || {
        update_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            2,
            WindowUnit::Hours,
            300,
        );
        let w = get_window(&env, &String::from_str(&env, "win1")).unwrap();
        assert_eq!(w.length, 2);
        assert_eq!(w.threshold_bps, 300);
        assert!(w.updated_at > w.created_at);
    });
}

#[test]
fn test_remove_window() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
    });
    env.as_contract(&contract_id, || {
        remove_window(&env, &admin, String::from_str(&env, "win1"));
        assert!(get_window(&env, &String::from_str(&env, "win1")).is_none());
        assert_eq!(get_all_windows(&env).len(), 0);
    });
}

#[test]
fn test_evaluate_no_breach() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        // 2% deviation, threshold 5% (500 bps) — no breach
        let eval =
            evaluate_threshold(&env, &String::from_str(&env, "win1"), 1_000_000, 1_020_000)
                .unwrap();
        assert!(!eval.is_breached);
    });
}

#[test]
fn test_evaluate_breach() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        // 10% deviation, threshold 5% (500 bps) — breach; breach_bps = 1000
        let eval =
            evaluate_threshold(&env, &String::from_str(&env, "win1"), 1_000_000, 1_100_000)
                .unwrap();
        assert!(eval.is_breached);
        assert_eq!(eval.breach_bps, 1_000);
    });
}

#[test]
fn test_evaluate_zero_reference() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
        let eval = evaluate_threshold(&env, &String::from_str(&env, "win1"), 0, 100).unwrap();
        assert!(!eval.is_breached);
        assert_eq!(eval.breach_bps, 0);
    });
}

#[test]
fn test_window_seconds_seconds_unit() {
    let env = Env::default();
    let config = WindowConfig {
        window_id: String::from_str(&env, "s"),
        length: 45,
        unit: WindowUnit::Seconds,
        threshold_bps: 100,
        created_at: 0,
        updated_at: 0,
    };
    assert_eq!(get_window_seconds(&config), 45);
}

#[test]
fn test_window_seconds_minutes_unit() {
    let env = Env::default();
    let config = WindowConfig {
        window_id: String::from_str(&env, "m"),
        length: 2,
        unit: WindowUnit::Minutes,
        threshold_bps: 100,
        created_at: 0,
        updated_at: 0,
    };
    assert_eq!(get_window_seconds(&config), 120);
}

#[test]
fn test_window_seconds_hours_unit() {
    let env = Env::default();
    let config = WindowConfig {
        window_id: String::from_str(&env, "h"),
        length: 3,
        unit: WindowUnit::Hours,
        threshold_bps: 100,
        created_at: 0,
        updated_at: 0,
    };
    assert_eq!(get_window_seconds(&config), 10_800);
}

#[test]
#[should_panic(expected = "maximum number of windows reached")]
fn test_max_windows_limit() {
    let (env, admin, contract_id) = setup();

    // Each create_window calls require_auth() — needs its own as_contract frame
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win0"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win1"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win2"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win3"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win4"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win5"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win6"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win7"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win8"), 1, WindowUnit::Hours, 100);
    });
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win9"), 1, WindowUnit::Hours, 100);
    });
    // 11th window exceeds MAX_WINDOWS (10) — should panic
    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, "win10"), 1, WindowUnit::Hours, 100);
    });
}

#[test]
#[should_panic(expected = "window already exists")]
fn test_create_duplicate_panics() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
    });
    // Second create with the same id should panic "window already exists"
    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            500,
        );
    });
}

#[test]
#[should_panic(expected = "window not found")]
fn test_update_nonexistent_panics() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        update_window(
            &env,
            &admin,
            String::from_str(&env, "nonexistent"),
            1,
            WindowUnit::Hours,
            500,
        );
    });
}

#[test]
#[should_panic(expected = "window not found")]
fn test_remove_nonexistent_panics() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        remove_window(&env, &admin, String::from_str(&env, "nonexistent"));
    });
}

#[test]
#[should_panic(expected = "window not found")]
fn test_evaluate_nonexistent_panics() {
    let (env, _admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        evaluate_threshold(&env, &String::from_str(&env, "nonexistent"), 1_000, 1_100);
    });
}

#[test]
#[should_panic(expected = "window_id cannot be empty")]
fn test_create_empty_id_panics() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(&env, &admin, String::from_str(&env, ""), 1, WindowUnit::Hours, 500);
    });
}

#[test]
#[should_panic(expected = "window length must be greater than 0")]
fn test_create_zero_length_panics() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            0,
            WindowUnit::Hours,
            500,
        );
    });
}

#[test]
#[should_panic(expected = "threshold must be between 1 and 10_000 bps")]
fn test_create_zero_threshold_panics() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            0,
        );
    });
}

#[test]
#[should_panic(expected = "threshold must be between 1 and 10_000 bps")]
fn test_create_over_10000_threshold_panics() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win1"),
            1,
            WindowUnit::Hours,
            10_001,
        );
    });
}

#[test]
fn test_threshold_counting_multiple_windows() {
    let (env, admin, contract_id) = setup();

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win_a"),
            1,
            WindowUnit::Hours,
            500,
        );
    });
    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "win_b"),
            30,
            WindowUnit::Minutes,
            200,
        );
    });

    env.as_contract(&contract_id, || {
        // 10% deviation exceeds both thresholds (5% and 2%)
        let eval_a =
            evaluate_threshold(&env, &String::from_str(&env, "win_a"), 1_000_000, 1_100_000)
                .unwrap();
        let eval_b =
            evaluate_threshold(&env, &String::from_str(&env, "win_b"), 1_000_000, 1_100_000)
                .unwrap();

        let breached_count = [eval_a.is_breached, eval_b.is_breached]
            .iter()
            .filter(|&&b| b)
            .count();
        assert_eq!(breached_count, 2);
    });
}
