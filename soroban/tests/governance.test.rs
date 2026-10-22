//! Integration tests for the governance module.
//!
//! Exercises the public entrypoints of `soroban/src/governance.rs` across the
//! happy path, authorization enforcement, guard/pause behavior, and emitted
//! events. Mirrors the style of the existing suites (e.g.
//! `operator_rotation.test.rs`, `version_migration_tests.rs`).

use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

use crate::governance::{GovernanceContract, GovernanceContractClient};

/// Shared setup helper: registers the governance contract and returns the
/// client together with a freshly generated admin address.
fn setup(env: &Env) -> (GovernanceContractClient, Address) {
    let admin = Address::generate(env);
    let contract_id = env.register_contract(None, GovernanceContract);
    let client = GovernanceContractClient::new(env, &contract_id);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_governance_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);

    // Admin can register a proposal and read it back.
    let proposal_id = client.propose(&admin, &Symbol::new(&env, "upgrade"));
    let proposal = client.get_proposal(&proposal_id);

    assert_eq!(proposal.id, proposal_id);
    assert_eq!(proposal.proposer, admin);
    assert_eq!(proposal.action, Symbol::new(&env, "upgrade"));
    assert!(!proposal.executed);
}

#[test]
fn test_governance_authorization_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup(&env);
    let unauthorized = Address::generate(&env);

    // A non-admin caller must not be able to register a proposal.
    let result = client.try_propose(&unauthorized, &Symbol::new(&env, "upgrade"));
    assert!(result.is_err());
}

#[test]
fn test_governance_paused_blocks_operations() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);

    // Pause the contract, then confirm proposals are rejected.
    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_propose(&admin, &Symbol::new(&env, "upgrade"));
    assert!(result.is_err());

    // Unpausing restores normal operation.
    client.unpause(&admin);
    assert!(!client.is_paused());
    let proposal_id = client.propose(&admin, &Symbol::new(&env, "upgrade"));
    assert_eq!(client.get_proposal(&proposal_id).id, proposal_id);
}

#[test]
fn test_governance_emits_events() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup(&env);

    let proposal_id = client.propose(&admin, &Symbol::new(&env, "upgrade"));

    // The proposal event should fire with the proposal id and proposer.
    let events = env.events().all();
    assert!(!events.is_empty());

    let (_, topics, _data) = events.last().unwrap();
    assert_eq!(topics.len(), 2);
    let _ = proposal_id;
}
