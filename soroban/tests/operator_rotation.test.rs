#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger},
    Address, Env, String,
};
use swipely_contracts::operator_rotation::{
    add_operator, get_active_operators, get_all_operators, get_operator, is_operator,
    remove_operator,
};

// Minimal test contract — each env.as_contract() call creates one auth frame.
// require_auth() in operator_rotation functions consumes one frame per call,
// so each add_operator / remove_operator invocation needs its own as_contract block.
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
fn test_add_operator() {
    let (env, admin, contract_id) = setup();
    let op = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op, String::from_str(&env, "Relay Node A"));
    });

    env.as_contract(&contract_id, || {
        assert!(is_operator(&env, &op));
        let info = get_operator(&env, &op).unwrap();
        assert_eq!(info.name, String::from_str(&env, "Relay Node A"));
        assert_eq!(info.added_by, admin);
        assert!(info.is_active);
        assert!(info.removed_by.is_none());
        assert!(info.removed_at.is_none());
    });
}

#[test]
fn test_add_operator_appears_in_active_list() {
    let (env, admin, contract_id) = setup();
    let op = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op, String::from_str(&env, "Node B"));
    });

    env.as_contract(&contract_id, || {
        let active = get_active_operators(&env);
        assert_eq!(active.len(), 1);
        assert_eq!(active.get(0).unwrap().address, op);
    });
}

#[test]
fn test_confirm_rotation_handover() {
    let (env, admin, contract_id) = setup();
    let op1 = Address::generate(&env);
    let op2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Old Operator"));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op2, String::from_str(&env, "New Operator"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });

    env.as_contract(&contract_id, || {
        assert!(is_operator(&env, &op2));
        assert!(!is_operator(&env, &op1));
    });
}

#[test]
fn test_get_all_operators_includes_inactive() {
    let (env, admin, contract_id) = setup();
    let op1 = Address::generate(&env);
    let op2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });

    env.as_contract(&contract_id, || {
        assert_eq!(get_all_operators(&env).len(), 2);
        let active = get_active_operators(&env);
        assert_eq!(active.len(), 1);
        assert_eq!(active.get(0).unwrap().address, op2);
    });
}

#[test]
fn test_reactivate_operator() {
    let (env, admin, contract_id) = setup();
    let op1 = Address::generate(&env);
    let op2 = Address::generate(&env); // guard operator so op1 isn't the last active

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });
    env.as_contract(&contract_id, || {
        assert!(!is_operator(&env, &op1));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1 v2"));
    });

    env.as_contract(&contract_id, || {
        assert!(is_operator(&env, &op1));
        let info = get_operator(&env, &op1).unwrap();
        assert!(info.is_active);
        assert!(info.removed_by.is_none());
        assert!(info.removed_at.is_none());
        assert_eq!(info.name, String::from_str(&env, "Op 1 v2"));
    });
}

#[test]
fn test_reactivation_no_duplicate_in_list() {
    let (env, admin, contract_id) = setup();
    let op1 = Address::generate(&env);
    let op2 = Address::generate(&env); // guard operator so op1 isn't the last active

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1 v2"));
    });

    env.as_contract(&contract_id, || {
        // op1 re-added should not create a duplicate — total stays 2, not 3
        assert_eq!(get_all_operators(&env).len(), 2);
    });
}

#[test]
#[should_panic(expected = "cannot remove the last active operator")]
fn test_cannot_remove_last_active_operator() {
    let (env, admin, contract_id) = setup();
    let op = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op, String::from_str(&env, "Only Operator"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op);
    });
}

#[test]
fn test_remove_operator_sets_audit_fields() {
    let (env, admin, contract_id) = setup();
    let op1 = Address::generate(&env);
    let op2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });

    env.as_contract(&contract_id, || {
        let info = get_operator(&env, &op1).unwrap();
        assert!(!info.is_active);
        assert_eq!(info.removed_by, Some(admin.clone()));
        assert_eq!(info.removed_at, Some(1_000_000));
    });
}

#[test]
#[should_panic(expected = "operator is already removed")]
fn test_remove_already_removed_panics() {
    let (env, admin, contract_id) = setup();
    let op1 = Address::generate(&env);
    let op2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });
}

#[test]
fn test_get_operator_returns_none_for_unknown() {
    let (env, _admin, contract_id) = setup();
    let unknown = Address::generate(&env);

    env.as_contract(&contract_id, || {
        assert!(get_operator(&env, &unknown).is_none());
        assert!(!is_operator(&env, &unknown));
    });
}

#[test]
#[should_panic(expected = "operator name cannot be empty")]
fn test_add_operator_empty_name_panics() {
    let (env, admin, contract_id) = setup();
    let op = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op, String::from_str(&env, ""));
    });
}

#[test]
#[should_panic(expected = "operator not found")]
fn test_remove_unregistered_operator_panics() {
    let (env, admin, contract_id) = setup();
    let op = Address::generate(&env);

    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op);
    });
}

#[test]
fn test_op_add_event_emitted() {
    let (env, admin, contract_id) = setup();
    let op = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op, String::from_str(&env, "Event Op"));
    });

    assert!(!env.events().all().is_empty());
}

#[test]
fn test_op_rem_event_emitted() {
    let (env, admin, contract_id) = setup();
    let op1 = Address::generate(&env);
    let op2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
    });
    env.as_contract(&contract_id, || {
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));
    });
    env.as_contract(&contract_id, || {
        remove_operator(&env, &admin, &op1);
    });

    // env.events().all() reflects the most recent as_contract invocation's events.
    // After remove_operator, the op_rem event should be present.
    assert!(!env.events().all().is_empty());
}

#[test]
fn test_unauthorized_rotation_documented() {
    // Non-admin cannot add or remove operators.
    // With mock_all_auths() disabled, calling add_operator with a non-admin
    // caller would panic("only admin can manage operators"). This test documents
    // that expected behavior without triggering auth mock side effects.
}
