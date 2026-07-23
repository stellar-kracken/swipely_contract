#![cfg(test)]
//! Resource-usage benchmarks for the escrow contract's lock/release hot path.
//!
//! Run with:
//!
//! ```bash
//! cargo test -p escrow-contract --test benchmarks -- --nocapture
//! ```
//!
//! Each `bench_*` test performs exactly one top-level contract invocation and
//! prints the resources Soroban's test host metered for it (CPU instructions,
//! memory, ledger footprint, and an estimated fee). See `BENCHMARKS.md` at
//! the repo root for how to interpret the numbers and the recorded baseline.

use escrow_contract::{TimeLockedEscrowContract, TimeLockedEscrowContractClient};
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

#[contract]
pub struct MockBridgeVerifier;

#[contractimpl]
impl MockBridgeVerifier {
    pub fn set_verified(env: Env, admin: Address, reference: String, verified: bool) {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&(symbol_short!("ref"), reference), &verified);
    }

    pub fn is_verified(env: Env, reference: String) -> bool {
        env.storage()
            .instance()
            .get(&(symbol_short!("ref"), reference))
            .unwrap_or(false)
    }
}

fn setup() -> (Env, TimeLockedEscrowContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register(TimeLockedEscrowContract, ());
    let client = TimeLockedEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let approver_1 = Address::generate(&env);
    let approver_2 = Address::generate(&env);
    let mut approvers = Vec::new(&env);
    approvers.push_back(approver_1);
    approvers.push_back(approver_2);

    client.initialize(&admin, &fee_collector, &100u32, &approvers, &2u32);

    (env, client, admin)
}

fn verifier(env: &Env) -> (Address, MockBridgeVerifierClient<'static>, Address) {
    let verifier_admin = Address::generate(env);
    let verifier_id = env.register(MockBridgeVerifier, ());
    let verifier_client = MockBridgeVerifierClient::new(env, &verifier_id);
    (verifier_id, verifier_client, verifier_admin)
}

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
fn bench_escrow_create_escrow() {
    let (env, client, _admin) = setup();
    let (verifier_id, _verifier_client, _verifier_admin) = verifier(&env);
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.create_escrow(
        &depositor,
        &recipient,
        &String::from_str(&env, "stellar-eth"),
        &String::from_str(&env, "USDC"),
        &100_000i128,
        &String::from_str(&env, "tx:bench"),
        &verifier_id,
        &String::from_str(&env, "proof:bench"),
    );

    report("escrow_contract::create_escrow", &env);
}

#[test]
fn bench_escrow_release_escrow() {
    let (env, client, _admin) = setup();
    let (verifier_id, verifier_client, verifier_admin) = verifier(&env);
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id = client.create_escrow(
        &depositor,
        &recipient,
        &String::from_str(&env, "stellar-eth"),
        &String::from_str(&env, "USDC"),
        &100_000i128,
        &String::from_str(&env, "tx:bench"),
        &verifier_id,
        &String::from_str(&env, "proof:bench"),
    );
    verifier_client.set_verified(
        &verifier_admin,
        &String::from_str(&env, "proof:bench"),
        &true,
    );
    client.sync_verification(
        &verifier_id,
        &escrow_id,
        &verifier_client.is_verified(&String::from_str(&env, "proof:bench")),
    );
    env.ledger().set_timestamp(1_000_000 + 3_600);

    client.release_escrow(&recipient, &escrow_id, &50_000i128);

    report("escrow_contract::release_escrow", &env);
}
