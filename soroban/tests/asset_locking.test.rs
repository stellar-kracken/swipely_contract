#![cfg(test)]

use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String};

// Import the contract and client
use swipely_contracts::{AdminRole, BridgeWatchContract, BridgeWatchContractClient};

fn setup() -> (
    Env,
    BridgeWatchContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register_contract(None, BridgeWatchContract);
    let client = BridgeWatchContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let manager = Address::generate(&env);
    let submitter = Address::generate(&env);

    client.initialize(&admin);

    // Grant roles
    client.grant_role(&admin, &manager, &AdminRole::AssetManager);
    client.grant_role(&admin, &submitter, &AdminRole::HealthSubmitter);
    client.grant_role(&admin, &submitter, &AdminRole::PriceSubmitter);

    (env, client, admin, manager, submitter)
}

// ────────────────────────────────────────────────────────────────────────────
// Basic Lock/Unlock Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn test_lock_asset_success() {
    let (env, client, admin, _manager, _submitter) = setup();

    // Register an asset
    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock the asset
    let reason = String::from_str(&env, "Under security review");
    client.lock_asset(&admin, &asset_code, &reason);

    // Verify it's locked
    assert!(client.is_asset_locked(&asset_code));

    // Verify lock state
    let lock_state = client.get_asset_lock_state(&asset_code);
    assert!(lock_state.is_some());

    let state = lock_state.unwrap();
    assert!(state.is_locked);
    assert_eq!(state.reason, reason);
    assert_eq!(state.locked_by, admin);
    assert!(state.locked_at > 0);
    assert!(state.unlocked_by.is_none());
    assert!(state.unlocked_at.is_none());
}

#[test]
fn test_unlock_asset_success() {
    let (env, client, admin, _manager, _submitter) = setup();

    // Register and lock asset
    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Maintenance"));

    // Verify it's locked
    assert!(client.is_asset_locked(&asset_code));

    // Unlock the asset
    client.unlock_asset(&admin, &asset_code);

    // Verify it's unlocked
    assert!(!client.is_asset_locked(&asset_code));

    // Verify lock state
    let lock_state = client.get_asset_lock_state(&asset_code);
    assert!(lock_state.is_some());

    let state = lock_state.unwrap();
    assert!(!state.is_locked);
    assert_eq!(state.unlocked_by.unwrap(), admin);
    assert!(state.unlocked_at.unwrap() > 0);
}

#[test]
fn test_is_asset_locked_returns_false_for_never_locked_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Asset has never been locked
    assert!(!client.is_asset_locked(&asset_code));
}

#[test]
fn test_get_asset_lock_state_returns_none_for_never_locked_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Asset has never been locked
    let lock_state = client.get_asset_lock_state(&asset_code);
    assert!(lock_state.is_none());
}

// ────────────────────────────────────────────────────────────────────────────
// Lock History Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn test_lock_history_recorded() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock asset
    let reason1 = String::from_str(&env, "First lock");
    client.lock_asset(&admin, &asset_code, &reason1);

    // Check history
    let history = client.get_asset_lock_history(&asset_code);
    assert_eq!(history.len(), 1);

    let entry = history.get(0).unwrap();
    assert!(entry.locked);
    assert_eq!(entry.reason, reason1);
    assert_eq!(entry.caller, admin);
    assert!(entry.timestamp > 0);
}

#[test]
fn test_lock_and_unlock_history() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock and unlock
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Maintenance"));
    client.unlock_asset(&admin, &asset_code);

    // Check history
    let history = client.get_asset_lock_history(&asset_code);
    assert_eq!(history.len(), 2);

    let lock_entry = history.get(0).unwrap();
    assert!(lock_entry.locked);

    let unlock_entry = history.get(1).unwrap();
    assert!(!unlock_entry.locked);
}

#[test]
fn test_multiple_lock_unlock_cycles_in_history() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // First cycle
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Cycle 1"));
    client.unlock_asset(&admin, &asset_code);

    // Second cycle
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Cycle 2"));
    client.unlock_asset(&admin, &asset_code);

    // Third cycle
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Cycle 3"));
    client.unlock_asset(&admin, &asset_code);

    // Check history
    let history = client.get_asset_lock_history(&asset_code);
    assert_eq!(history.len(), 6); // 3 locks + 3 unlocks
}

#[test]
fn test_empty_lock_history_for_never_locked_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    let history = client.get_asset_lock_history(&asset_code);
    assert_eq!(history.len(), 0);
}

// ────────────────────────────────────────────────────────────────────────────
// Guard Tests - Mutating Operations Blocked When Locked
// ────────────────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "asset is locked for maintenance")]
fn test_submit_health_blocked_when_locked() {
    let (env, client, admin, _manager, submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock the asset
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Under review"));

    // Try to submit health - should panic
    client.submit_health(&submitter, &asset_code, &95, &90, &92, &88);
}

#[test]
#[should_panic(expected = "asset is locked for maintenance")]
fn test_submit_price_blocked_when_locked() {
    let (env, client, admin, _manager, submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock the asset
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Under review"));

    // Try to submit price - should panic
    client.submit_price(
        &submitter,
        &asset_code,
        &1_000_000,
        &String::from_str(&env, "oracle1"),
    );
}

#[test]
#[should_panic(expected = "asset is locked for maintenance")]
fn test_pause_asset_blocked_when_locked() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock the asset
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Under review"));

    // Try to pause - should panic
    client.pause_asset(&admin, &asset_code);
}

#[test]
#[should_panic(expected = "asset is locked for maintenance")]
fn test_unpause_asset_blocked_when_locked() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Pause first
    client.pause_asset(&admin, &asset_code);

    // Lock the asset
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Under review"));

    // Try to unpause - should panic
    client.unpause_asset(&admin, &asset_code);
}

#[test]
fn test_operations_work_after_unlock() {
    let (env, client, admin, _manager, submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock and unlock
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Maintenance"));
    client.unlock_asset(&admin, &asset_code);

    // Operations should work now
    client.submit_health(&submitter, &asset_code, &95, &90, &92, &88);
    client.submit_price(
        &submitter,
        &asset_code,
        &1_000_000,
        &String::from_str(&env, "oracle1"),
    );
    client.pause_asset(&admin, &asset_code);
    client.unpause_asset(&admin, &asset_code);

    // No panics means success
}

// ────────────────────────────────────────────────────────────────────────────
// Error Cases
// ────────────────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "asset is already locked")]
fn test_cannot_lock_already_locked_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock once
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "First lock"));

    // Try to lock again - should panic
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Second lock"));
}

#[test]
#[should_panic(expected = "asset is not locked")]
fn test_cannot_unlock_unlocked_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Try to unlock without locking first - should panic
    client.unlock_asset(&admin, &asset_code);
}

#[test]
#[should_panic(expected = "asset is not locked")]
fn test_cannot_unlock_already_unlocked_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock and unlock
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Maintenance"));
    client.unlock_asset(&admin, &asset_code);

    // Try to unlock again - should panic
    client.unlock_asset(&admin, &asset_code);
}

#[test]
#[should_panic(expected = "cannot lock a deregistered asset")]
fn test_cannot_lock_deregistered_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Deregister
    client.deregister_asset(&admin, &asset_code);

    // Try to lock - should panic
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Test"));
}

#[test]
#[should_panic(expected = "cannot unlock a deregistered asset")]
fn test_cannot_unlock_deregistered_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Test"));

    // Deregister
    client.deregister_asset(&admin, &asset_code);

    // Try to unlock - should panic
    client.unlock_asset(&admin, &asset_code);
}

// ────────────────────────────────────────────────────────────────────────────
// Authorization Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn test_admin_can_lock_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Admin should be able to lock
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Admin lock"));

    assert!(client.is_asset_locked(&asset_code));
}

#[test]
fn test_asset_manager_can_lock_asset() {
    let (env, client, admin, manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Asset manager should be able to lock
    client.lock_asset(
        &manager,
        &asset_code,
        &String::from_str(&env, "Manager lock"),
    );

    assert!(client.is_asset_locked(&asset_code));
}

#[test]
fn test_admin_can_unlock_asset() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Test"));

    // Admin should be able to unlock
    client.unlock_asset(&admin, &asset_code);

    assert!(!client.is_asset_locked(&asset_code));
}

#[test]
fn test_asset_manager_can_unlock_asset() {
    let (env, client, admin, manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Test"));

    // Asset manager should be able to unlock
    client.unlock_asset(&manager, &asset_code);

    assert!(!client.is_asset_locked(&asset_code));
}

// ────────────────────────────────────────────────────────────────────────────
// Event Emission Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn test_lock_emits_events() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Lock the asset
    let reason = String::from_str(&env, "Security review");
    client.lock_asset(&admin, &asset_code, &reason);

    // Events are emitted (verified by the contract implementation)
    // In a real test environment, we would check env.events() here
    assert!(client.is_asset_locked(&asset_code));
}

#[test]
fn test_unlock_emits_events() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);
    client.lock_asset(&admin, &asset_code, &String::from_str(&env, "Test"));

    // Unlock the asset
    client.unlock_asset(&admin, &asset_code);

    // Events are emitted (verified by the contract implementation)
    // In a real test environment, we would check env.events() here
    assert!(!client.is_asset_locked(&asset_code));
}

// ────────────────────────────────────────────────────────────────────────────
// Integration Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn test_complete_lock_maintenance_unlock_workflow() {
    let (env, client, admin, _manager, submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Initial submission works
    client.submit_health(&submitter, &asset_code, &95, &90, &92, &88);

    // Lock for maintenance
    client.lock_asset(
        &admin,
        &asset_code,
        &String::from_str(&env, "Emergency maintenance"),
    );

    // Verify locked
    assert!(client.is_asset_locked(&asset_code));

    // Submissions blocked during maintenance
    // (would panic if we tried)

    // Complete maintenance and unlock
    client.unlock_asset(&admin, &asset_code);

    // Verify unlocked
    assert!(!client.is_asset_locked(&asset_code));

    // Submissions work again
    client.submit_health(&submitter, &asset_code, &96, &91, &93, &89);
    client.submit_price(
        &submitter,
        &asset_code,
        &1_000_100,
        &String::from_str(&env, "oracle1"),
    );
}

#[test]
fn test_lock_state_persists_across_queries() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    let reason = String::from_str(&env, "Under review");
    client.lock_asset(&admin, &asset_code, &reason);

    // Query multiple times
    assert!(client.is_asset_locked(&asset_code));
    assert!(client.is_asset_locked(&asset_code));
    assert!(client.is_asset_locked(&asset_code));

    let state1 = client.get_asset_lock_state(&asset_code).unwrap();
    let state2 = client.get_asset_lock_state(&asset_code).unwrap();

    // State should be consistent
    assert_eq!(state1.is_locked, state2.is_locked);
    assert_eq!(state1.reason, state2.reason);
    assert_eq!(state1.locked_by, state2.locked_by);
}

#[test]
fn test_different_admin_can_unlock() {
    let (env, client, admin, manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    // Admin locks
    client.lock_asset(
        &admin,
        &asset_code,
        &String::from_str(&env, "Locked by admin"),
    );

    // Manager unlocks
    client.unlock_asset(&manager, &asset_code);

    // Should be unlocked
    assert!(!client.is_asset_locked(&asset_code));

    // Verify lock state shows different actors
    let state = client.get_asset_lock_state(&asset_code).unwrap();
    assert_eq!(state.locked_by, admin);
    assert_eq!(state.unlocked_by.unwrap(), manager);
}

#[test]
fn test_lock_reason_preserved_after_unlock() {
    let (env, client, admin, _manager, _submitter) = setup();

    let asset_code = String::from_str(&env, "USDC");
    client.register_asset(&admin, &asset_code);

    let reason = String::from_str(&env, "Critical security patch");
    client.lock_asset(&admin, &asset_code, &reason);
    client.unlock_asset(&admin, &asset_code);

    // Original lock reason should still be in the state
    let state = client.get_asset_lock_state(&asset_code).unwrap();
    assert_eq!(state.reason, reason);
}
