//! Integration tests for the `submission_pause` module.
//!
//! Exercises the public entrypoints of the submission pause contract across the
//! happy path, authorization enforcement, pause guard behavior, and emitted
//! events. Mirrors the style of the existing suites (e.g.
//! `operator_rotation.test.rs`, `version_migration_tests.rs`).

use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

use submission_pause::{SubmissionPause, SubmissionPauseClient};

/// Deploy the contract and return the client plus an admin address.
fn setup(env: &Env) -> (SubmissionPauseClient<'_>, Address) {
    let contract_id = env.register_contract(None, SubmissionPause);
    let client = SubmissionPauseClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_initialize_sets_admin_and_unpaused() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);

    assert_eq!(client.get_admin(), admin);
    assert!(!client.is_paused());
}

#[test]
fn test_initialize_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SubmissionPause);
    let client = SubmissionPauseClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let events = env.events().all();
    assert!(!events.is_empty());
    let (_, topics, _) = events.last().unwrap();
    assert_eq!(topics, (Symbol::new(&env, "init"),).into_val(&env));
}

#[test]
fn test_pause_and_unpause_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);

    client.pause();
    assert!(client.is_paused());

    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_pause_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);

    client.pause();

    // The admin must have authorized the pause call.
    let auths = env.auths();
    assert!(auths.iter().any(|(addr, _)| addr == &admin));
}

#[test]
#[should_panic]
fn test_pause_rejects_unauthorized_caller() {
    let env = Env::default();

    let (client, _admin) = setup(&env);

    // No auths mocked: the admin gate must reject the call.
    client.pause();
}

#[test]
#[should_panic]
fn test_unpause_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.pause();

    // Fresh env without mocked auths for the unpause call.
    let env2 = Env::default();
    let (client2, _admin2) = setup(&env2);
    client2.unpause();
}

#[test]
fn test_pause_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);

    client.pause();

    let events = env.events().all();
    let (_, topics, _) = events.last().unwrap();
    assert_eq!(topics, (Symbol::new(&env, "pause"),).into_val(&env));
}

#[test]
fn test_unpause_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    client.pause();

    client.unpause();

    let events = env.events().all();
    let (_, topics, _) = events.last().unwrap();
    assert_eq!(topics, (Symbol::new(&env, "unpause"),).into_val(&env));
}

#[test]
fn test_pause_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);

    client.pause();
    client.pause();

    assert!(client.is_paused());
}

#[test]
fn test_unpause_when_not_paused_is_noop() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);

    client.unpause();

    assert!(!client.is_paused());
}
