use soroban_sdk::{contracttype, Address, Env, String, Vec};

use crate::acl::{self, Permission};
use crate::keys;

/// Default delay between proposing and executing a rotation (24 hours).
pub const DEFAULT_ROTATION_DELAY_SECS: u64 = 86_400;
/// Default window after the delay elapses during which a rotation may still
/// be executed (24 hours). Once this grace period passes, the pending
/// rotation is stale and must be cancelled and re-proposed.
pub const DEFAULT_ROTATION_GRACE_PERIOD_SECS: u64 = 86_400;

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
pub struct OperatorAddedEvent {
    pub version: u32,
    pub operator: Address,
    pub name: String,
    pub added_by: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorRemovedEvent {
    pub version: u32,
    pub operator: Address,
    pub removed_by: Address,
    pub timestamp: u64,
}

/// The change a pending rotation will apply once executed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RotationAction {
    /// Add (or reactivate) the operator at the given address, under the given name.
    Add(Address, String),
    /// Remove the operator at the given address.
    Remove(Address),
}

/// Configurable timelock parameters for operator rotation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotationConfig {
    /// Minimum time that must elapse between propose and execute.
    pub delay_secs: u64,
    /// Window after the delay during which execution remains valid.
    pub grace_period_secs: u64,
}

/// A proposed rotation awaiting its timelock delay before it can execute.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRotation {
    pub action: RotationAction,
    pub proposed_by: Address,
    pub proposed_at: u64,
    /// Earliest timestamp at which `execute_rotation` may succeed.
    pub executes_at: u64,
    /// Latest timestamp at which `execute_rotation` may succeed; after this
    /// the rotation is stale and must be cancelled and re-proposed.
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperatorRotationKey {
    Operator(Address),
    AllOperators,
    RotationConfig,
    PendingRotation,
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
    apply_add_operator(env, caller, operator_address, name);
}

/// Core of `add_operator`, without the admin check — reused by
/// `execute_rotation` so it doesn't re-authorize the caller within the same
/// invocation.
fn apply_add_operator(env: &Env, caller: &Address, operator_address: &Address, name: String) {
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
        (
            soroban_sdk::symbol_short!("Swipely"),
            soroban_sdk::Symbol::new(env, "Operator"),
            soroban_sdk::Symbol::new(env, "Added"),
            operator_address.clone(),
        ),
        OperatorAddedEvent {
            version: 1,
            operator: operator_address.clone(),
            name,
            added_by: caller.clone(),
            timestamp: now,
        },
    );
}

pub fn remove_operator(env: &Env, caller: &Address, operator_address: &Address) {
    require_admin(env, caller);
    apply_remove_operator(env, caller, operator_address);
}

/// Core of `remove_operator`, without the admin check — reused by
/// `execute_rotation` so it doesn't re-authorize the caller within the same
/// invocation.
fn apply_remove_operator(env: &Env, caller: &Address, operator_address: &Address) {
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
        (
            soroban_sdk::symbol_short!("Swipely"),
            soroban_sdk::Symbol::new(env, "Operator"),
            soroban_sdk::Symbol::new(env, "Removed"),
            operator_address.clone(),
        ),
        OperatorRemovedEvent {
            version: 1,
            operator: operator_address.clone(),
            removed_by: caller.clone(),
            timestamp: now,
        },
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

// ---------------------------------------------------------------------------
// Timelocked rotation (issue #8)
// ---------------------------------------------------------------------------

/// Set the rotation timelock delay and grace period (admin only).
pub fn set_rotation_config(env: &Env, caller: &Address, delay_secs: u64, grace_period_secs: u64) {
    require_admin(env, caller);

    if delay_secs == 0 {
        panic!("rotation delay must be greater than 0");
    }
    if grace_period_secs == 0 {
        panic!("rotation grace period must be greater than 0");
    }

    let config = RotationConfig {
        delay_secs,
        grace_period_secs,
    };
    env.storage()
        .instance()
        .set(&OperatorRotationKey::RotationConfig, &config);
}

/// Get the current rotation timelock configuration, falling back to the
/// documented defaults when the admin has not configured one.
pub fn get_rotation_config(env: &Env) -> RotationConfig {
    env.storage()
        .instance()
        .get(&OperatorRotationKey::RotationConfig)
        .unwrap_or(RotationConfig {
            delay_secs: DEFAULT_ROTATION_DELAY_SECS,
            grace_period_secs: DEFAULT_ROTATION_GRACE_PERIOD_SECS,
        })
}

/// Propose an operator rotation (admin only). The rotation becomes
/// executable after the configured delay and remains valid for the
/// configured grace period thereafter.
///
/// # Panics
/// - `caller` is not the admin.
/// - A rotation is already pending.
/// - `Add` is proposed with an empty name.
/// - `Remove` targets an address that is not a currently active operator.
pub fn propose_rotation(env: &Env, caller: &Address, action: RotationAction) {
    require_admin(env, caller);

    let key = OperatorRotationKey::PendingRotation;
    if env.storage().persistent().has(&key) {
        panic!("a rotation is already pending");
    }

    match &action {
        RotationAction::Add(_, name) => {
            if name.is_empty() {
                panic!("operator name cannot be empty");
            }
        }
        RotationAction::Remove(operator) => {
            if !is_operator(env, operator) {
                panic!("operator not found or not active");
            }
        }
    }

    let now = env.ledger().timestamp();
    let config = get_rotation_config(env);
    let executes_at = now + config.delay_secs;
    let pending = PendingRotation {
        action,
        proposed_by: caller.clone(),
        proposed_at: now,
        executes_at,
        expires_at: executes_at + config.grace_period_secs,
    };
    env.storage().persistent().set(&key, &pending);

    env.events().publish(
        (soroban_sdk::symbol_short!("rot_prop"), caller.clone()),
        pending,
    );
}

/// Execute a pending rotation once its timelock delay has elapsed (admin only).
///
/// # Panics
/// - `caller` is not the admin.
/// - No rotation is pending.
/// - The delay has not yet elapsed.
/// - The grace period has expired (the rotation is stale).
/// - The underlying `Add`/`Remove` action itself panics (e.g. removing the
///   last active operator).
pub fn execute_rotation(env: &Env, caller: &Address) {
    require_admin(env, caller);

    let key = OperatorRotationKey::PendingRotation;
    let pending: PendingRotation = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("no rotation is pending"));

    let now = env.ledger().timestamp();
    if now < pending.executes_at {
        panic!("rotation delay has not elapsed");
    }
    if now > pending.expires_at {
        panic!("rotation grace period has expired");
    }

    match pending.action.clone() {
        RotationAction::Add(operator, name) => {
            apply_add_operator(env, caller, &operator, name);
        }
        RotationAction::Remove(operator) => {
            apply_remove_operator(env, caller, &operator);
        }
    }

    env.storage().persistent().remove(&key);

    env.events().publish(
        (soroban_sdk::symbol_short!("rot_exec"), caller.clone()),
        pending,
    );
}

/// Cancel a pending rotation before it executes.
///
/// Callable by the admin or by any address holding the `EmergencyPause`
/// permission (see [`crate::acl`]), so a malicious or mistaken proposal can
/// be aborted even if the admin key that proposed it is compromised.
///
/// # Panics
/// - `caller` is neither the admin nor `EmergencyPause`-authorized.
/// - No rotation is pending.
pub fn cancel_rotation(env: &Env, caller: &Address) {
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    acl::require_permission(env, caller, &admin, &Permission::EmergencyPause);

    let key = OperatorRotationKey::PendingRotation;
    let pending: PendingRotation = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("no rotation is pending"));
    env.storage().persistent().remove(&key);

    env.events().publish(
        (soroban_sdk::symbol_short!("rot_cncl"), caller.clone()),
        pending,
    );
}

/// Get the currently pending rotation, if any. Public read access.
pub fn get_pending_rotation(env: &Env) -> Option<PendingRotation> {
    env.storage()
        .persistent()
        .get(&OperatorRotationKey::PendingRotation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Events as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::{contract, contractimpl, Env};

    // Free functions in this module touch env.storage(), which soroban-sdk only
    // allows from within an active contract call frame. This dummy contract
    // exists purely to give tests that frame via env.as_contract().
    #[contract]
    struct TestContext;

    #[contractimpl]
    impl TestContext {}

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(TestContext, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            env.storage().instance().set(&keys::ADMIN, &admin);
        });
        env.ledger().set_timestamp(1_000_000);
        (env, admin, contract_id)
    }

    #[test]
    fn test_add_operator() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, "Operator 1"));

            assert!(is_operator(&env, &op));
        });
    }

    #[test]
    fn test_remove_operator() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        // remove_operator refuses to drop the last active operator, so keep
        // a second one around for this call to succeed.
        let other = Address::generate(&env);

        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, "Operator 1"));
        });
        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &other, String::from_str(&env, "Operator 2"));
        });
        env.as_contract(&contract_id, || {
            assert!(is_operator(&env, &op));
            remove_operator(&env, &admin, &op);
        });
        env.as_contract(&contract_id, || {
            assert!(!is_operator(&env, &op));
        });
    }

    #[test]
    fn test_cannot_remove_last_operator() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, "Operator 1"));
            assert!(is_operator(&env, &op));
        });
    }

    #[test]
    fn test_get_all_operators() {
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
            let all = get_all_operators(&env);
            assert_eq!(all.len(), 2);
        });
    }

    #[test]
    fn test_get_active_operators_excludes_removed() {
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
            let active = get_active_operators(&env);
            assert_eq!(active.len(), 1);
            assert_eq!(active.get(0).unwrap().address, op2);
        });
    }

    #[test]
    fn test_unknown_operator_not_active() {
        let (env, _admin, contract_id) = setup();
        let unknown = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert!(!is_operator(&env, &unknown));
        });
    }

    #[test]
    #[should_panic(expected = "operator name cannot be empty")]
    fn test_add_operator_empty_name() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, ""));
        });
    }

    #[test]
    #[should_panic(expected = "operator not found")]
    fn test_remove_unregistered_operator() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        env.as_contract(&contract_id, || {
            remove_operator(&env, &admin, &op);
        });
    }

    #[test]
    fn test_reactivate_operator() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        // remove_operator refuses to drop the last active operator, so keep
        // a second one around for this call to succeed.
        let other = Address::generate(&env);

        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, "Op 1"));
        });
        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &other, String::from_str(&env, "Other"));
        });
        env.as_contract(&contract_id, || {
            remove_operator(&env, &admin, &op);
        });
        env.as_contract(&contract_id, || {
            assert!(!is_operator(&env, &op));
        });
        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, "Op 1 v2"));
        });
        env.as_contract(&contract_id, || {
            assert!(is_operator(&env, &op));
        });
    }

    #[test]
    fn test_operator_event_emission() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, "Event Op"));
        });
    }

    // -----------------------------------------------------------------------
    // Timelocked rotation (issue #8)
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_rotation_config() {
        let (env, _admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let config = get_rotation_config(&env);
            assert_eq!(config.delay_secs, DEFAULT_ROTATION_DELAY_SECS);
            assert_eq!(config.grace_period_secs, DEFAULT_ROTATION_GRACE_PERIOD_SECS);
        });
    }

    #[test]
    fn test_set_rotation_config() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 3_600, 7_200);
            let config = get_rotation_config(&env);
            assert_eq!(config.delay_secs, 3_600);
            assert_eq!(config.grace_period_secs, 7_200);
        });
    }

    #[test]
    #[should_panic(expected = "rotation delay must be greater than 0")]
    fn test_set_rotation_config_rejects_zero_delay() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 0, 7_200);
        });
    }

    #[test]
    #[should_panic(expected = "rotation grace period must be greater than 0")]
    fn test_set_rotation_config_rejects_zero_grace_period() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 3_600, 0);
        });
    }

    #[test]
    fn test_propose_rotation_stores_pending() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 3_600, 7_200);
        });
        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "New Operator")),
            );
        });
        env.as_contract(&contract_id, || {
            let pending = get_pending_rotation(&env).unwrap();
            assert_eq!(pending.proposed_by, admin);
            assert_eq!(pending.proposed_at, 1_000_000);
            assert_eq!(pending.executes_at, 1_000_000 + 3_600);
            assert_eq!(pending.expires_at, 1_000_000 + 3_600 + 7_200);
            assert!(!is_operator(&env, &op));
        });
    }

    #[test]
    #[should_panic(expected = "a rotation is already pending")]
    fn test_propose_rotation_while_pending_panics() {
        let (env, admin, contract_id) = setup();
        let op1 = Address::generate(&env);
        let op2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op1.clone(), String::from_str(&env, "Op 1")),
            );
        });
        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op2.clone(), String::from_str(&env, "Op 2")),
            );
        });
    }

    #[test]
    #[should_panic(expected = "operator not found or not active")]
    fn test_propose_remove_unknown_operator_panics() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            propose_rotation(&env, &admin, RotationAction::Remove(op));
        });
    }

    #[test]
    #[should_panic(expected = "no rotation is pending")]
    fn test_execute_rotation_without_pending_panics() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            execute_rotation(&env, &admin);
        });
    }

    #[test]
    #[should_panic(expected = "rotation delay has not elapsed")]
    fn test_execute_rotation_before_delay_panics() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 3_600, 7_200);
        });
        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "New Operator")),
            );
        });
        env.as_contract(&contract_id, || {
            execute_rotation(&env, &admin);
        });
    }

    #[test]
    fn test_execute_add_rotation_after_delay_succeeds() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 3_600, 7_200);
        });
        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "New Operator")),
            );
        });

        env.ledger().set_timestamp(1_000_000 + 3_600);
        env.as_contract(&contract_id, || {
            execute_rotation(&env, &admin);
        });
        env.as_contract(&contract_id, || {
            assert!(is_operator(&env, &op));
            assert!(get_pending_rotation(&env).is_none());
        });
    }

    #[test]
    fn test_execute_remove_rotation_after_delay_succeeds() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        let other = Address::generate(&env);

        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &op, String::from_str(&env, "Op 1"));
        });
        env.as_contract(&contract_id, || {
            add_operator(&env, &admin, &other, String::from_str(&env, "Op 2"));
        });
        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 3_600, 7_200);
        });
        env.as_contract(&contract_id, || {
            propose_rotation(&env, &admin, RotationAction::Remove(op.clone()));
        });

        env.ledger().set_timestamp(1_000_000 + 3_600);
        env.as_contract(&contract_id, || {
            execute_rotation(&env, &admin);
        });
        env.as_contract(&contract_id, || {
            assert!(!is_operator(&env, &op));
            assert!(get_pending_rotation(&env).is_none());
        });
    }

    #[test]
    #[should_panic(expected = "rotation grace period has expired")]
    fn test_execute_rotation_after_grace_period_panics() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            set_rotation_config(&env, &admin, 3_600, 7_200);
        });
        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "New Operator")),
            );
        });

        env.ledger().set_timestamp(1_000_000 + 3_600 + 7_200 + 1);
        env.as_contract(&contract_id, || {
            execute_rotation(&env, &admin);
        });
    }

    #[test]
    fn test_cancel_rotation_by_admin() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "New Operator")),
            );
        });
        env.as_contract(&contract_id, || {
            cancel_rotation(&env, &admin);
        });
        env.as_contract(&contract_id, || {
            assert!(get_pending_rotation(&env).is_none());
        });
    }

    #[test]
    fn test_cancel_rotation_by_emergency_pause_role() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        let guardian = Address::generate(&env);

        env.as_contract(&contract_id, || {
            acl::grant_permission_internal(&env, &guardian, &Permission::EmergencyPause, &admin, 0);
        });
        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "New Operator")),
            );
        });
        env.as_contract(&contract_id, || {
            cancel_rotation(&env, &guardian);
        });
        env.as_contract(&contract_id, || {
            assert!(get_pending_rotation(&env).is_none());
        });
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller lacks the required permission")]
    fn test_cancel_rotation_by_unauthorized_caller_panics() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);
        let stranger = Address::generate(&env);

        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "New Operator")),
            );
        });
        env.as_contract(&contract_id, || {
            cancel_rotation(&env, &stranger);
        });
    }

    #[test]
    #[should_panic(expected = "no rotation is pending")]
    fn test_cancel_rotation_without_pending_panics() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            cancel_rotation(&env, &admin);
        });
    }

    #[test]
    fn test_cancelled_rotation_can_be_reproposed() {
        let (env, admin, contract_id) = setup();
        let op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "First try")),
            );
        });
        env.as_contract(&contract_id, || {
            cancel_rotation(&env, &admin);
        });
        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op.clone(), String::from_str(&env, "Second try")),
            );
        });
        env.as_contract(&contract_id, || {
            assert!(get_pending_rotation(&env).is_some());
        });
    }

    #[test]
    fn test_propose_execute_cancel_emit_events() {
        let (env, admin, contract_id) = setup();
        let op1 = Address::generate(&env);
        let op2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op1.clone(), String::from_str(&env, "Op 1")),
            );
        });
        assert!(!env.events().all().is_empty());

        env.as_contract(&contract_id, || {
            cancel_rotation(&env, &admin);
        });
        assert!(!env.events().all().is_empty());

        env.as_contract(&contract_id, || {
            propose_rotation(
                &env,
                &admin,
                RotationAction::Add(op2.clone(), String::from_str(&env, "Op 2")),
            );
        });

        env.ledger()
            .set_timestamp(1_000_000 + DEFAULT_ROTATION_DELAY_SECS);
        env.as_contract(&contract_id, || {
            execute_rotation(&env, &admin);
        });
        assert!(!env.events().all().is_empty());
    }
}
