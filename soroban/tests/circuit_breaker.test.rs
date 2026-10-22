//! Integration tests for the `circuit_breaker` module.
//!
//! These tests exercise the module's public entrypoints end-to-end:
//! the happy path, authorization enforcement, breaker-open guard
//! behavior, and emitted events. They mirror the style of the existing
//! suites (e.g. `operator_rotation.test.rs`, `version_migration_tests.rs`)
//! and are fully deterministic so they run in the standard `cargo test` job.

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

use soroban::circuit_breaker::{CircuitBreaker, CircuitBreakerClient, BreakerState};

/// Shared setup helper: deploy the circuit breaker contract and return a
/// client plus the admin address used to initialize it.
fn setup<'a>(env: &'a Env) -> (CircuitBreakerClient<'a>, Address) {
    let admin = Address::generate(env);
    let contract_id = env.register_contract(None, CircuitBreaker);
    let client = CircuitBreakerClient::new(env, &contract_id);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn happy_path_initialize_and_trip() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);

    // Freshly initialized breaker is closed.
    assert_eq!(client.state(), BreakerState::Closed);

    // Admin can trip the breaker.
    client.trip(&admin);
    assert_eq!(client.state(), BreakerState::Open);

    // Admin can reset it back to closed.
    client.reset(&admin);
    assert_eq!(client.state(), BreakerState::Closed);
}

#[test]
fn authorization_rejects_unauthorized_trip() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    let stranger = Address::generate(&env);

    // A non-admin caller must not be able to trip the breaker.
    let result = client.try_trip(&stranger);
    assert!(result.is_err());
    assert_eq!(client.state(), BreakerState::Closed);
}

#[test]
fn authorization_rejects_unauthorized_reset() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);
    client.trip(&admin);

    let stranger = Address::generate(&env);
    let result = client.try_reset(&stranger);
    assert!(result.is_err());
    assert_eq!(client.state(), BreakerState::Open);
}

#[test]
fn guard_blocks_operations_when_open() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);
    client.trip(&admin);

    // While the breaker is open, guarded operations must be rejected.
    let result = client.try_check_and_run(&admin);
    assert!(result.is_err());
    assert_eq!(client.state(), BreakerState::Open);
}

#[test]
fn guard_allows_operations_when_closed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);

    // While closed, guarded operations succeed.
    let result = client.try_check_and_run(&admin);
    assert!(result.is_ok());
    assert_eq!(client.state(), BreakerState::Closed);
}

#[test]
fn emits_event_on_trip() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);
    client.trip(&admin);

    let events = env.events().all();
    let expected_topic: Vec<Val> = (Symbol::new(&env, "breaker_tripped"),).into_val(&env);

    let found = events.iter().any(|(_, topics, _)| topics == expected_topic);
    assert!(found, "expected breaker_tripped event to be emitted");
}

#[test]
fn emits_event_on_reset() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);
    client.trip(&admin);
    client.reset(&admin);

    let events = env.events().all();
    let expected_topic: Vec<Val> = (Symbol::new(&env, "breaker_reset"),).into_val(&env);

    let found = events.iter().any(|(_, topics, _)| topics == expected_topic);
    assert!(found, "expected breaker_reset event to be emitted");
}
