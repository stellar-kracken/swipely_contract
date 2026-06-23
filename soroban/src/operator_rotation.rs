use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::keys;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Operator {
    pub address: Address,
    pub name: String,
    pub added_by: Address,
    pub added_at: u64,
    pub is_active: bool,
    pub removed_by: Option<Address>,
    pub removed_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorEntry {
    pub address: Address,
    pub name: String,
    pub is_active: bool,
    pub added_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperatorRotationKey {
    Operator(Address),
    AllOperators,
}

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    if *caller != admin {
        panic!("only admin can manage operators");
    }
}

pub fn add_operator(env: &Env, caller: &Address, operator_address: &Address, name: String) {
    require_admin(env, caller);

    if name.is_empty() {
        panic!("operator name cannot be empty");
    }

    let now = env.ledger().timestamp();
    let key = OperatorRotationKey::Operator(operator_address.clone());
    let existing: Option<Operator> = env.storage().persistent().get(&key);

    let operator = match existing {
        Some(mut existing_op) => {
            existing_op.is_active = true;
            existing_op.name = name.clone();
            existing_op.added_by = caller.clone();
            existing_op.added_at = now;
            existing_op.removed_by = None;
            existing_op.removed_at = None;
            existing_op
        }
        None => Operator {
            address: operator_address.clone(),
            name: name.clone(),
            added_by: caller.clone(),
            added_at: now,
            is_active: true,
            removed_by: None,
            removed_at: None,
        },
    };

    env.storage().persistent().set(&key, &operator);

    let all_key = OperatorRotationKey::AllOperators;
    let mut all: Vec<Address> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut found = false;
    for addr in all.iter() {
        if &addr == operator_address {
            found = true;
            break;
        }
    }

    if !found {
        all.push_back(operator_address.clone());
        env.storage().persistent().set(&all_key, &all);
    }

    env.events().publish(
        (symbol_short!("op_add"),),
        (operator_address.clone(), name, caller.clone(), now),
    );
}

pub fn remove_operator(env: &Env, caller: &Address, operator_address: &Address) {
    require_admin(env, caller);

    let key = OperatorRotationKey::Operator(operator_address.clone());
    let mut operator: Operator = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("operator not found"));

    if !operator.is_active {
        panic!("operator is already removed");
    }

    let all_key = OperatorRotationKey::AllOperators;
    let all: Vec<Address> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut active_count = 0u32;
    for addr in all.iter() {
        if &addr == operator_address {
            continue;
        }
        let op_key = OperatorRotationKey::Operator(addr.clone());
        if let Some(op) = env.storage().persistent().get::<_, Operator>(&op_key) {
            if op.is_active {
                active_count += 1;
            }
        }
    }

    if active_count == 0 {
        panic!("cannot remove the last active operator");
    }

    let now = env.ledger().timestamp();
    operator.is_active = false;
    operator.removed_by = Some(caller.clone());
    operator.removed_at = Some(now);

    env.storage().persistent().set(&key, &operator);

    env.events().publish(
        (symbol_short!("op_rem"),),
        (operator_address.clone(), caller.clone(), now),
    );
}

pub fn is_operator(env: &Env, operator_address: &Address) -> bool {
    let key = OperatorRotationKey::Operator(operator_address.clone());
    let operator: Option<Operator> = env.storage().persistent().get(&key);
    match operator {
        Some(op) => op.is_active,
        None => false,
    }
}

pub fn get_operator(env: &Env, operator_address: &Address) -> Option<Operator> {
    let key = OperatorRotationKey::Operator(operator_address.clone());
    env.storage().persistent().get(&key)
}

pub fn get_all_operators(env: &Env) -> Vec<OperatorEntry> {
    let all_key = OperatorRotationKey::AllOperators;
    let all: Vec<Address> = env
        .storage()
        .persistent()
        .get(&all_key)
        .unwrap_or_else(|| Vec::new(env));

    let mut result: Vec<OperatorEntry> = Vec::new(env);
    for addr in all.iter() {
        let key = OperatorRotationKey::Operator(addr.clone());
        if let Some(op) = env.storage().persistent().get::<_, Operator>(&key) {
            result.push_back(OperatorEntry {
                address: op.address,
                name: op.name,
                is_active: op.is_active,
                added_at: op.added_at,
            });
        }
    }
    result
}

pub fn get_active_operators(env: &Env) -> Vec<OperatorEntry> {
    let all = get_all_operators(env);
    let mut result: Vec<OperatorEntry> = Vec::new(env);
    for entry in all.iter() {
        if entry.is_active {
            result.push_back(entry);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&keys::ADMIN, &admin);
        env.ledger().set_timestamp(1_000_000);
        (env, admin)
    }

    #[test]
    fn test_add_operator() {
        let (env, admin) = setup();
        let op = Address::generate(&env);

        add_operator(&env, &admin, &op, String::from_str(&env, "Operator 1"));

        assert!(is_operator(&env, &op));
    }

    #[test]
    fn test_remove_operator() {
        let (env, admin) = setup();
        let op = Address::generate(&env);

        add_operator(&env, &admin, &op, String::from_str(&env, "Operator 1"));
        assert!(is_operator(&env, &op));

        remove_operator(&env, &admin, &op);
        assert!(!is_operator(&env, &op));
    }

    #[test]
    fn test_cannot_remove_last_operator() {
        let (env, admin) = setup();
        let op = Address::generate(&env);

        add_operator(&env, &admin, &op, String::from_str(&env, "Operator 1"));
        assert!(is_operator(&env, &op));
    }

    #[test]
    fn test_get_all_operators() {
        let (env, admin) = setup();
        let op1 = Address::generate(&env);
        let op2 = Address::generate(&env);

        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));

        let all = get_all_operators(&env);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_active_operators_excludes_removed() {
        let (env, admin) = setup();
        let op1 = Address::generate(&env);
        let op2 = Address::generate(&env);

        add_operator(&env, &admin, &op1, String::from_str(&env, "Op 1"));
        add_operator(&env, &admin, &op2, String::from_str(&env, "Op 2"));
        remove_operator(&env, &admin, &op1);

        let active = get_active_operators(&env);
        assert_eq!(active.len(), 1);
        assert_eq!(active.get(0).unwrap().address, op2);
    }

    #[test]
    fn test_unknown_operator_not_active() {
        let (env, _admin) = setup();
        let unknown = Address::generate(&env);
        assert!(!is_operator(&env, &unknown));
    }

    #[test]
    #[should_panic(expected = "operator name cannot be empty")]
    fn test_add_operator_empty_name() {
        let (env, admin) = setup();
        let op = Address::generate(&env);
        add_operator(&env, &admin, &op, String::from_str(&env, ""));
    }

    #[test]
    #[should_panic(expected = "operator not found")]
    fn test_remove_unregistered_operator() {
        let (env, admin) = setup();
        let op = Address::generate(&env);
        remove_operator(&env, &admin, &op);
    }

    #[test]
    fn test_reactivate_operator() {
        let (env, admin) = setup();
        let op = Address::generate(&env);

        add_operator(&env, &admin, &op, String::from_str(&env, "Op 1"));
        remove_operator(&env, &admin, &op);
        assert!(!is_operator(&env, &op));

        add_operator(&env, &admin, &op, String::from_str(&env, "Op 1 v2"));
        assert!(is_operator(&env, &op));
    }

    #[test]
    fn test_operator_event_emission() {
        let (env, admin) = setup();
        let op = Address::generate(&env);
        add_operator(&env, &admin, &op, String::from_str(&env, "Event Op"));
    }
}
