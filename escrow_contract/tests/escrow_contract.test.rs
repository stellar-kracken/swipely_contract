#![cfg(test)]

use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String, contract, contractimpl, symbol_short, Vec};
use escrow_contract::{TimeLockedEscrowContract, TimeLockedEscrowContractClient};

#[contract]
pub struct MockBridgeVerifier;

#[contractimpl]
impl MockBridgeVerifier {
    pub fn set_verified(env: Env, admin: Address, reference: String, verified: bool) {
        admin.require_auth();
        env.storage().instance().set(&(symbol_short!("ref"), reference), &verified);
    }

    pub fn is_verified(env: Env, reference: String) -> bool {
        env.storage()
            .instance()
            .get(&(symbol_short!("ref"), reference))
            .unwrap_or(false)
    }
}

fn setup() -> (
    Env,
    TimeLockedEscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register_contract(None, TimeLockedEscrowContract);
    let client = TimeLockedEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let approver_1 = Address::generate(&env);
    let approver_2 = Address::generate(&env);

    let mut approvers = Vec::new(&env);
    approvers.push_back(approver_1.clone());
    approvers.push_back(approver_2.clone());

    client.initialize(&admin, &fee_collector, &100u32, &approvers, &2u32);

    (env, client, admin, fee_collector, approver_1, approver_2)
}

fn verifier(env: &Env) -> (Address, MockBridgeVerifierClient<'static>, Address) {
    let verifier_admin = Address::generate(env);
    let verifier_id = env.register_contract(None, MockBridgeVerifier);
    let verifier_client = MockBridgeVerifierClient::new(env, &verifier_id);
    (verifier_id, verifier_client, verifier_admin)
}

#[test]
fn create_and_release_after_lock_and_verification() {
    let (env, client, admin, _fee_collector, _a1, _a2) = setup();
    let (verifier_id, verifier_client, verifier_admin) = verifier(&env);
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let bridge = String::from_str(&env, "stellar-eth");
    let asset = String::from_str(&env, "USDC");
    client.set_lock_period(&admin, &bridge, &asset, &120u64);

    let escrow_id = client
        .create_escrow(
            &depositor,
            &recipient,
            &bridge,
            &asset,
            &100_000i128,
            &String::from_str(&env, "tx:abc"),
            &verifier_id,
            &String::from_str(&env, "proof:1"),
        );

    let early = client.try_release_escrow(&recipient, &escrow_id, &99_000i128);
    assert!(early.is_err());

    verifier_client.set_verified(
        &verifier_admin,
        &String::from_str(&env, "proof:1"),
        &true,
    );
    client
        .sync_verification(&verifier_id, &escrow_id, &verifier_client.is_verified(&String::from_str(&env, "proof:1")));

    env.ledger().set_timestamp(1_000_130);
    let released = client.release_escrow(&recipient, &escrow_id, &50_000i128);
    assert_eq!(released, 50_000);

    let escrow = client.get_escrow(&escrow_id).unwrap();
    assert_eq!(escrow.released_amount, 50_000);
}

#[test]
fn challenge_then_multisig_resolve_release_path() {
    let (env, client, _admin, _fee_collector, approver_1, approver_2) = setup();
    let (verifier_id, verifier_client, verifier_admin) = verifier(&env);
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let challenger = Address::generate(&env);

    let escrow_id = client
        .create_escrow(
            &depositor,
            &recipient,
            &String::from_str(&env, "bridge-a"),
            &String::from_str(&env, "XLM"),
            &50_000,
            &String::from_str(&env, "meta"),
            &verifier_id,
            &String::from_str(&env, "proof:2"),
        );

    client.challenge_escrow(&challenger, &escrow_id, &String::from_str(&env, "suspicious"));

    let r1 = client.resolve_challenge(&approver_1, &escrow_id, &true);
    assert!(!r1);
    let r2 = client.resolve_challenge(&approver_2, &escrow_id, &true);
    assert!(r2);

    verifier_client.set_verified(&verifier_admin, &String::from_str(&env, "proof:2"), &true);
    client.sync_verification(&verifier_id, &escrow_id, &true);

    env.ledger().set_timestamp(1_004_000);
    let released = client.release_escrow(&recipient, &escrow_id, &49_500);
    assert_eq!(released, 49_500);
}

#[test]
fn dispute_reject_then_refund() {
    let (_env, client, _admin, _fee_collector, approver_1, approver_2) = setup();
    let (verifier_id, _verifier_client, _verifier_admin) = verifier(&_env);
    let depositor = Address::generate(&_env);
    let recipient = Address::generate(&_env);
    let challenger = Address::generate(&_env);

    let escrow_id = client
        .create_escrow(
            &depositor,
            &recipient,
            &String::from_str(&_env, "bridge-r"),
            &String::from_str(&_env, "USDT"),
            &10_000,
            &String::from_str(&_env, "meta"),
            &verifier_id,
            &String::from_str(&_env, "proof:3"),
        );
    client.challenge_escrow(&challenger, &escrow_id, &String::from_str(&_env, "bad"));

    client.resolve_challenge(&approver_1, &escrow_id, &false);
    client.resolve_challenge(&approver_2, &escrow_id, &false);

    let refunded = client.refund_escrow(&depositor, &escrow_id);
    assert_eq!(refunded, 9_900);
}

#[test]
fn batch_release_and_fee_collection() {
    let (env, client, admin, fee_collector, _a1, _a2) = setup();
    let (verifier_id, _verifier_client, _verifier_admin) = verifier(&env);
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.set_lock_period(
            &admin,
            &String::from_str(&env, "bridge-b"),
            &String::from_str(&env, "EURC"),
            &1,
        );

    let e1 = client
        .create_escrow(
            &depositor,
            &recipient,
            &String::from_str(&env, "bridge-b"),
            &String::from_str(&env, "EURC"),
            &1_000,
            &String::from_str(&env, "m1"),
            &verifier_id,
            &String::from_str(&env, "proof:4"),
        );
    let e2 = client
        .create_escrow(
            &depositor,
            &recipient,
            &String::from_str(&env, "bridge-b"),
            &String::from_str(&env, "EURC"),
            &2_000,
            &String::from_str(&env, "m2"),
            &verifier_id,
            &String::from_str(&env, "proof:5"),
        );

    client.sync_verification(&verifier_id, &e1, &true);
    client.sync_verification(&verifier_id, &e2, &true);
    env.ledger().set_timestamp(1_000_100);

    let mut ids = Vec::new(&env);
    ids.push_back(e1);
    ids.push_back(e2);
    let batch = client.batch_release(&recipient, &ids, &500);
    assert_eq!(batch.len(), 2);
    assert_eq!(client.get_accrued_fees(), 30);

    let collected = client.collect_fees(&fee_collector, &10);
    assert_eq!(collected, 10);
    assert_eq!(client.get_accrued_fees(), 20);
}

#[test]
fn emergency_recovery_requires_pause() {
    let (env, client, admin, _fee_collector, _a1, _a2) = setup();
    let (verifier_id, _verifier_client, _verifier_admin) = verifier(&env);
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id = client
        .create_escrow(
            &depositor,
            &recipient,
            &String::from_str(&env, "bridge-x"),
            &String::from_str(&env, "XLM"),
            &8_000,
            &String::from_str(&env, "meta"),
            &verifier_id,
            &String::from_str(&env, "proof:6"),
        );

    assert!(client
        .try_emergency_recover(&admin, &escrow_id, &recipient, &100)
        .is_err());

    client.set_emergency_pause(&admin, &true);
    client.emergency_recover(&admin, &escrow_id, &recipient, &500);

    let esc = client.get_escrow(&escrow_id).unwrap();
    assert_eq!(esc.released_amount, 500);
}
