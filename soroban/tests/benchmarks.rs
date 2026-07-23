#![cfg(test)]
//! Resource-usage benchmarks for the hottest `soroban` entry points.
//!
//! Run with:
//!
//! ```bash
//! cargo test -p swipely-contracts --test benchmarks -- --nocapture
//! ```
//!
//! Each `bench_*` test performs exactly one top-level contract invocation and
//! prints the resources Soroban's test host metered for it (CPU instructions,
//! memory, ledger footprint, and an estimated fee). See `BENCHMARKS.md` at
//! the repo root for how to interpret the numbers and the recorded baseline.

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, Env, String,
};
use swipely_contracts::threshold_window::{create_window, evaluate_threshold, WindowUnit};
use swipely_contracts::{BridgeWatchContract, BridgeWatchContractClient};

// Minimal test contract so `env.as_contract()` can provide a storage context
// for threshold_window's free functions (mirrors soroban/tests/threshold_window.test.rs).
#[contract]
struct BenchContext;
#[contractimpl]
impl BenchContext {}

/// Prints the resources metered for the last top-level contract invocation
/// on `env`, and sanity-checks that metering actually captured something.
fn report(name: &str, env: &Env) {
    let resources = env.cost_estimate().resources();
    let fee = env.cost_estimate().fee();
    println!(
        "{name:<40} instructions={:>10} mem_bytes={:>8} read_entries={:>3} write_entries={:>3} read_bytes={:>7} write_bytes={:>7} est_fee_stroops={:>8}",
        resources.instructions,
        resources.mem_bytes,
        resources.read_entries,
        resources.write_entries,
        resources.read_bytes,
        resources.write_bytes,
        fee.total,
    );
    assert!(
        resources.instructions > 0,
        "{name}: expected non-zero instructions — was cost metering captured?"
    );
}

#[test]
fn bench_threshold_window_create_window() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let admin = Address::generate(&env);
    let contract_id = env.register(BenchContext, ());
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&"admin", &admin);
    });

    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "price_dev_1h"),
            1,
            WindowUnit::Hours,
            500,
        );
    });

    report("threshold_window::create_window", &env);
}

#[test]
fn bench_threshold_window_evaluate_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let admin = Address::generate(&env);
    let contract_id = env.register(BenchContext, ());
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&"admin", &admin);
    });
    env.as_contract(&contract_id, || {
        create_window(
            &env,
            &admin,
            String::from_str(&env, "price_dev_1h"),
            1,
            WindowUnit::Hours,
            500,
        );
    });

    env.as_contract(&contract_id, || {
        evaluate_threshold(
            &env,
            &String::from_str(&env, "price_dev_1h"),
            1_000_000,
            1_020_000,
        );
    });

    report("threshold_window::evaluate_threshold", &env);
}

#[test]
fn bench_source_trust_register_trusted_source() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(BridgeWatchContract, ());
    let client = BridgeWatchContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let source = Address::generate(&env);
    client.initialize(&admin);

    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Benchmark Oracle"));

    report("source_trust::register_trusted_source", &env);
}

#[test]
fn bench_source_trust_revoke_trusted_source() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(BridgeWatchContract, ());
    let client = BridgeWatchContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let source = Address::generate(&env);
    client.initialize(&admin);
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Benchmark Oracle"));

    client.revoke_trusted_source(&admin, &source);

    report("source_trust::revoke_trusted_source", &env);
}
