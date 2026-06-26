#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

use bridge_watch_soroban::governance::{
    GovernanceContract, GovernanceContractClient, ProposalStatus, ProposalType, VoteChoice,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, GovernanceContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);

    let contract_id = env.register_contract(None, GovernanceContract);
    let client = GovernanceContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(
        &admin,
        &100,   // timelock_delay
        &200,   // voting_period
        &10,    // voting_delay
        &1_000, // quorum_bps (10 %)
        &5_100, // pass_threshold_bps (51 %)
        &100,   // proposal_deposit
        &false, // use_quadratic
        &1,     // guardian_threshold
    );

    (env, client, admin)
}

fn advance(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| li.timestamp += secs);
}

fn mk(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn funded_proposer(env: &Env, client: &GovernanceContractClient, power: i128) -> Address {
    let p = Address::generate(env);
    client.set_voting_power(&p, &power);
    p
}

fn create_proposal(
    env: &Env,
    client: &GovernanceContractClient,
    proposer: &Address,
    ptype: ProposalType,
) -> u32 {
    let target = Address::generate(env);
    client.create_proposal(
        proposer,
        &ptype,
        &mk(env, "title"),
        &mk(env, "description"),
        &target,
        &mk(env, "calldata"),
    )
}

// ── initialize ────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_stores_config() {
    let (env, client, _admin) = setup();
    let cfg = client.get_config();
    assert_eq!(cfg.quorum_bps, 1_000);
    assert_eq!(cfg.pass_threshold_bps, 5_100);
    assert_eq!(cfg.proposal_deposit, 100);
    assert!(!cfg.use_quadratic);
    assert_eq!(client.proposal_count(), 0);
    assert_eq!(client.total_supply(), 0);
}

// ── Proposal creation & activation ───────────────────────────────────────────

#[test]
fn test_create_proposal_starts_pending() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 500);

    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.status, ProposalStatus::Pending);
    assert_eq!(proposal.votes_for, 0);
    assert_eq!(proposal.votes_against, 0);
    assert_eq!(proposal.votes_abstain, 0);
}

#[test]
fn test_activate_proposal_after_delay() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 500);
    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    advance(&env, 15); // past voting_delay (10)
    client.activate_proposal(&id);

    assert_eq!(client.get_proposal(&id).status, ProposalStatus::Active);
}

#[test]
fn test_proposal_count_increments_per_proposal() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 500);

    assert_eq!(client.proposal_count(), 0);
    create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);
    assert_eq!(client.proposal_count(), 1);
    create_proposal(&env, &client, &proposer, ProposalType::OperatorApproval);
    assert_eq!(client.proposal_count(), 2);
}

// ── Full lifecycle: Pending → Active → Passed → Queued → Executed ─────────────

#[test]
fn test_full_lifecycle_passes_and_executes() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 500);
    let voter = funded_proposer(&env, &client, 500);
    let executor = Address::generate(&env);
    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    // Activate
    advance(&env, 15);
    client.activate_proposal(&id);

    // Both vote For (total supply = 1000, total votes = 1000 → quorum met, threshold met)
    client.cast_vote(&proposer, &id, &VoteChoice::For);
    client.cast_vote(&voter, &id, &VoteChoice::For);

    // Finalize after voting period
    advance(&env, 205);
    client.finalize_proposal(&id);
    assert_eq!(client.get_proposal(&id).status, ProposalStatus::Passed);

    // Queue for timelock
    client.queue_proposal(&id);
    assert_eq!(client.get_proposal(&id).status, ProposalStatus::Queued);

    // Execute after timelock
    advance(&env, 105);
    client.execute_proposal(&executor, &id);
    assert_eq!(client.get_proposal(&id).status, ProposalStatus::Executed);
}

// ── Quorum failure ────────────────────────────────────────────────────────────

#[test]
fn test_proposal_fails_when_quorum_not_met() {
    let (env, client, _admin) = setup();
    // Only proposer has power; no votes cast → total_votes = 0 < quorum
    let proposer = funded_proposer(&env, &client, 1_000);
    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    advance(&env, 15);
    client.activate_proposal(&id);
    advance(&env, 205);
    client.finalize_proposal(&id);

    assert_eq!(client.get_proposal(&id).status, ProposalStatus::Failed);
}

// ── Voting ────────────────────────────────────────────────────────────────────

#[test]
fn test_cast_vote_records_voting_power() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 200);
    let voter = funded_proposer(&env, &client, 300);
    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    advance(&env, 15);
    client.activate_proposal(&id);
    client.cast_vote(&voter, &id, &VoteChoice::Against);

    let record = client.get_vote(&id, &voter).unwrap();
    assert_eq!(record.voting_power, 300);

    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.votes_against, 300);
    assert_eq!(proposal.votes_for, 0);
}

#[test]
fn test_abstain_vote_counts_toward_quorum() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 500);
    let abstainer = funded_proposer(&env, &client, 500);
    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    advance(&env, 15);
    client.activate_proposal(&id);
    client.cast_vote(&abstainer, &id, &VoteChoice::Abstain);

    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.votes_abstain, 500);
}

#[test]
#[should_panic]
fn test_double_vote_panics() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 200);
    let voter = funded_proposer(&env, &client, 100);
    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    advance(&env, 15);
    client.activate_proposal(&id);
    client.cast_vote(&voter, &id, &VoteChoice::For);
    client.cast_vote(&voter, &id, &VoteChoice::Against); // should panic
}

// ── Cancellation ──────────────────────────────────────────────────────────────

#[test]
fn test_proposer_can_cancel() {
    let (env, client, _admin) = setup();
    let proposer = funded_proposer(&env, &client, 200);
    let id = create_proposal(&env, &client, &proposer, ProposalType::ParameterChange);

    client.cancel_proposal(&proposer, &id);
    assert_eq!(client.get_proposal(&id).status, ProposalStatus::Cancelled);
}

// ── Guardian multi-sig emergency execution ────────────────────────────────────

#[test]
fn test_guardian_emergency_execute() {
    let (env, client, _admin) = setup();
    let guardian = Address::generate(&env);
    client.add_guardian(&guardian);
    assert!(client.is_guardian(&guardian));

    let proposer = funded_proposer(&env, &client, 500);
    let id = create_proposal(&env, &client, &proposer, ProposalType::EmergencyPause);

    advance(&env, 15);
    client.activate_proposal(&id);

    // guardian_threshold = 1, so one approval is enough
    client.guardian_approve(&guardian, &id);
    assert_eq!(client.get_guardian_approvals(&id), 1);

    client.guardian_execute(&guardian, &id);
    assert_eq!(client.get_proposal(&id).status, ProposalStatus::Executed);
}

#[test]
#[should_panic]
fn test_non_guardian_cannot_approve() {
    let (env, client, _admin) = setup();
    let non_guardian = Address::generate(&env);
    let proposer = funded_proposer(&env, &client, 200);
    let id = create_proposal(&env, &client, &proposer, ProposalType::EmergencyPause);

    advance(&env, 15);
    client.activate_proposal(&id);
    client.guardian_approve(&non_guardian, &id); // should panic: "not a guardian"
}

// ── Vote delegation ───────────────────────────────────────────────────────────

#[test]
fn test_delegation_transfers_effective_power() {
    let (env, client, _admin) = setup();
    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);

    client.set_voting_power(&delegator, &400);

    assert_eq!(client.get_voting_power(&delegator), 400);
    assert_eq!(client.get_voting_power(&delegatee), 0);

    client.delegate_votes(&delegator, &delegatee);
    assert_eq!(client.get_voting_power(&delegator), 0);
    assert_eq!(client.get_voting_power(&delegatee), 400);

    let delegation = client.get_delegation(&delegator);
    assert!(delegation.is_some());
}

#[test]
fn test_undelegation_restores_power() {
    let (env, client, _admin) = setup();
    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);

    client.set_voting_power(&delegator, &400);
    client.delegate_votes(&delegator, &delegatee);
    client.undelegate_votes(&delegator);

    assert_eq!(client.get_voting_power(&delegator), 400);
    assert_eq!(client.get_voting_power(&delegatee), 0);
}
