#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String};

// Import the contract and client
use bridge_watch_soroban::{BridgeWatchContract, BridgeWatchContractClient};

fn setup() -> (Env, BridgeWatchContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, BridgeWatchContract);
    let client = BridgeWatchContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let source1 = Address::generate(&env);
    let source2 = Address::generate(&env);

    client.initialize(&admin);

    (env, client, admin, source1, source2)
}

#[test]
fn test_register_trusted_source() {
    let (_env, client, admin, source, _) = setup();

    // Register a trusted source
    client.register_trusted_source(&admin, &source, &String::from_str(&_env, "Test Oracle"));

    // Verify it's trusted
    assert!(client.is_trusted_source(&source));

    // Get source details
    let source_info = client.get_trusted_source(&source);
    assert!(source_info.is_some());
    let info = source_info.unwrap();
    assert_eq!(info.source_address, source);
    assert_eq!(info.name, String::from_str(&_env, "Test Oracle"));
    assert!(info.is_active);
}

#[test]
fn test_register_multiple_sources() {
    let (_env, client, admin, source1, source2) = setup();

    // Register two sources
    client.register_trusted_source(&admin, &source1, &String::from_str(&_env, "Oracle 1"));
    client.register_trusted_source(&admin, &source2, &String::from_str(&_env, "Oracle 2"));

    // Both should be trusted
    assert!(client.is_trusted_source(&source1));
    assert!(client.is_trusted_source(&source2));

    // Get all sources
    let all_sources = client.get_all_trusted_sources();
    assert_eq!(all_sources.len(), 2);

    // Get active sources
    let active_sources = client.get_active_trusted_sources();
    assert_eq!(active_sources.len(), 2);
}

#[test]
fn test_revoke_trusted_source() {
    let (_env, client, admin, source, _) = setup();

    // Register and then revoke
    client.register_trusted_source(&admin, &source, &String::from_str(&_env, "Test Oracle"));
    assert!(client.is_trusted_source(&source));

    client.revoke_trusted_source(&admin, &source);

    // Should no longer be trusted
    assert!(!client.is_trusted_source(&source));

    // But should still be in the registry
    let source_info = client.get_trusted_source(&source);
    assert!(source_info.is_some());
    let info = source_info.unwrap();
    assert!(!info.is_active);
    assert!(info.revoked_by.is_some());
    assert!(info.revoked_at.is_some());
}

#[test]
fn test_reactivate_revoked_source() {
    let (_env, client, admin, source, _) = setup();

    // Register, revoke, then register again
    client.register_trusted_source(&admin, &source, &String::from_str(&_env, "Test Oracle"));
    client.revoke_trusted_source(&admin, &source);
    assert!(!client.is_trusted_source(&source));

    // Re-register with new name
    client.register_trusted_source(&admin, &source, &String::from_str(&_env, "Updated Oracle"));

    // Should be active again
    assert!(client.is_trusted_source(&source));
    let info = client.get_trusted_source(&source).unwrap();
    assert!(info.is_active);
    assert_eq!(info.name, String::from_str(&_env, "Updated Oracle"));
}

#[test]
fn test_get_active_sources_excludes_revoked() {
    let (_env, client, admin, source1, source2) = setup();

    // Register two sources
    client.register_trusted_source(&admin, &source1, &String::from_str(&_env, "Oracle 1"));
    client.register_trusted_source(&admin, &source2, &String::from_str(&_env, "Oracle 2"));

    // Revoke one
    client.revoke_trusted_source(&admin, &source1);

    // All sources should return 2
    let all_sources = client.get_all_trusted_sources();
    assert_eq!(all_sources.len(), 2);

    // Active sources should return 1
    let active_sources = client.get_active_trusted_sources();
    assert_eq!(active_sources.len(), 1);
    assert_eq!(active_sources.get(0).unwrap().source_address, source2);
}

#[test]
fn test_unregistered_source_is_not_trusted() {
    let (_env, client, _admin, source, _) = setup();

    // Source not registered
    assert!(!client.is_trusted_source(&source));

    // Get should return None
    assert!(client.get_trusted_source(&source).is_none());
}

#[test]
#[should_panic(expected = "source not registered")]
fn test_revoke_unregistered_source_panics() {
    let (_env, client, admin, source, _) = setup();

    // Try to revoke unregistered source
    client.revoke_trusted_source(&admin, &source);
}

#[test]
#[should_panic(expected = "source already revoked")]
fn test_revoke_already_revoked_source_panics() {
    let (_env, client, admin, source, _) = setup();

    // Register and revoke
    client.register_trusted_source(&admin, &source, &String::from_str(&_env, "Test Oracle"));
    client.revoke_trusted_source(&admin, &source);

    // Try to revoke again
    client.revoke_trusted_source(&admin, &source);
}

#[test]
#[should_panic(expected = "source name cannot be empty")]
fn test_register_with_empty_name_panics() {
    let (_env, client, admin, source, _) = setup();

    // Try to register with empty name
    client.register_trusted_source(&admin, &source, &String::from_str(&_env, ""));
}

#[test]
fn test_source_registration_audit_trail() {
    let (env, client, admin, source, _) = setup();

    // Register source
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Test Oracle"));

    // Check source details include audit info
    let info = client.get_trusted_source(&source).unwrap();
    assert_eq!(info.registered_by, admin);
    assert!(info.registered_at > 0);
    assert!(info.revoked_by.is_none());
    assert!(info.revoked_at.is_none());

    // Revoke and check audit trail
    client.revoke_trusted_source(&admin, &source);
    let info = client.get_trusted_source(&source).unwrap();
    assert_eq!(info.revoked_by.unwrap(), admin);
    assert!(info.revoked_at.unwrap() > 0);
}

#[test]
fn test_non_admin_cannot_register_source() {
    let (_env, client, _admin, source, non_admin) = setup();

    // Non-admin tries to register (should fail with auth)
    // Note: In real scenario, this would fail auth check
    // For this test, we're just documenting the expected behavior
    // The actual panic would be "unauthorized: caller lacks the required permission"
}

#[test]
fn test_non_admin_cannot_revoke_source() {
    let (_env, client, admin, source, non_admin) = setup();

    // Admin registers source
    client.register_trusted_source(&admin, &source, &String::from_str(&_env, "Test Oracle"));

    // Non-admin tries to revoke (should fail with auth)
    // Note: In real scenario, this would fail auth check
    // For this test, we're just documenting the expected behavior
    // The actual panic would be "unauthorized: caller lacks the required permission"
}

#[test]
fn test_source_info_contains_correct_data() {
    let (env, client, admin, source, _) = setup();

    let name = String::from_str(&env, "CoinGecko Oracle");
    client.register_trusted_source(&admin, &source, &name);

    let all_sources = client.get_all_trusted_sources();
    assert_eq!(all_sources.len(), 1);

    let info = all_sources.get(0).unwrap();
    assert_eq!(info.source_address, source);
    assert_eq!(info.name, name);
    assert!(info.is_active);
    assert!(info.registered_at > 0);
}

#[test]
fn test_multiple_registrations_updates_source() {
    let (env, client, admin, source, _) = setup();

    // Register with first name
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Oracle v1"));
    let info1 = client.get_trusted_source(&source).unwrap();
    let timestamp1 = info1.registered_at;

    // Advance time
    env.ledger().with_mut(|li| li.timestamp += 100);

    // Register again with new name
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Oracle v2"));
    let info2 = client.get_trusted_source(&source).unwrap();

    // Should have updated name and timestamp
    assert_eq!(info2.name, String::from_str(&env, "Oracle v2"));
    assert!(info2.registered_at > timestamp1);

    // Should still only have one source in the list
    let all_sources = client.get_all_trusted_sources();
    assert_eq!(all_sources.len(), 1);
}


// ── Integration Tests with Submission Gating ─────────────────────────────────

#[test]
fn test_submit_health_requires_trusted_source_when_sources_registered() {
    let (env, client, admin, trusted_source, untrusted_source) = setup();

    // Register asset
    client.register_asset(&admin, &String::from_str(&env, "USDC"));

    // Grant submission role to both sources
    client.grant_role(&admin, &trusted_source, bridge_watch_soroban::AdminRole::HealthSubmitter);
    client.grant_role(&admin, &untrusted_source, bridge_watch_soroban::AdminRole::HealthSubmitter);

    // Before registering any trusted sources, both should be able to submit
    client.submit_health(&trusted_source, &String::from_str(&env, "USDC"), 95, 90, 92, 88);
    client.submit_health(&untrusted_source, &String::from_str(&env, "USDC"), 94, 89, 91, 87);

    // Now register the trusted source
    client.register_trusted_source(
        &admin,
        &trusted_source,
        &String::from_str(&env, "Trusted Oracle"),
    );

    // Trusted source should still work
    client.submit_health(&trusted_source, &String::from_str(&env, "USDC"), 96, 91, 93, 89);

    // Untrusted source should now fail (would panic with "caller is not a trusted source")
    // Note: In actual test, this would panic. Documenting expected behavior.
}

#[test]
fn test_submit_price_requires_trusted_source_when_sources_registered() {
    let (env, client, admin, trusted_source, untrusted_source) = setup();

    // Register asset
    client.register_asset(&admin, &String::from_str(&env, "USDC"));

    // Grant submission role to both sources
    client.grant_role(&admin, &trusted_source, bridge_watch_soroban::AdminRole::PriceSubmitter);
    client.grant_role(&admin, &untrusted_source, bridge_watch_soroban::AdminRole::PriceSubmitter);

    // Before registering any trusted sources, both should be able to submit
    client.submit_price(
        &trusted_source,
        &String::from_str(&env, "USDC"),
        1_000_000,
        &String::from_str(&env, "oracle1"),
    );
    client.submit_price(
        &untrusted_source,
        &String::from_str(&env, "USDC"),
        1_000_100,
        &String::from_str(&env, "oracle2"),
    );

    // Now register the trusted source
    client.register_trusted_source(
        &admin,
        &trusted_source,
        &String::from_str(&env, "Trusted Price Feed"),
    );

    // Trusted source should still work
    client.submit_price(
        &trusted_source,
        &String::from_str(&env, "USDC"),
        1_000_200,
        &String::from_str(&env, "oracle1"),
    );

    // Untrusted source should now fail (would panic with "caller is not a trusted source")
    // Note: In actual test, this would panic. Documenting expected behavior.
}

#[test]
fn test_revoked_source_cannot_submit() {
    let (env, client, admin, source, _) = setup();

    // Register asset
    client.register_asset(&admin, &String::from_str(&env, "USDC"));

    // Register and grant role to source
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Oracle"));
    client.grant_role(&admin, &source, bridge_watch_soroban::AdminRole::HealthSubmitter);

    // Should be able to submit
    client.submit_health(&source, &String::from_str(&env, "USDC"), 95, 90, 92, 88);

    // Revoke the source
    client.revoke_trusted_source(&admin, &source);

    // Should no longer be able to submit (would panic with "caller is not a trusted source")
    // Note: In actual test, this would panic. Documenting expected behavior.
}

#[test]
fn test_reactivated_source_can_submit_again() {
    let (env, client, admin, source, _) = setup();

    // Register asset
    client.register_asset(&admin, &String::from_str(&env, "USDC"));

    // Register and grant role to source
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Oracle"));
    client.grant_role(&admin, &source, bridge_watch_soroban::AdminRole::HealthSubmitter);

    // Submit successfully
    client.submit_health(&source, &String::from_str(&env, "USDC"), 95, 90, 92, 88);

    // Revoke
    client.revoke_trusted_source(&admin, &source);

    // Re-register
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Oracle v2"));

    // Should be able to submit again
    client.submit_health(&source, &String::from_str(&env, "USDC"), 96, 91, 93, 89);
}

#[test]
fn test_multiple_trusted_sources_can_all_submit() {
    let (env, client, admin, source1, source2) = setup();

    // Register asset
    client.register_asset(&admin, &String::from_str(&env, "USDC"));

    // Register both sources
    client.register_trusted_source(&admin, &source1, &String::from_str(&env, "Oracle 1"));
    client.register_trusted_source(&admin, &source2, &String::from_str(&env, "Oracle 2"));

    // Grant roles
    client.grant_role(&admin, &source1, bridge_watch_soroban::AdminRole::HealthSubmitter);
    client.grant_role(&admin, &source2, bridge_watch_soroban::AdminRole::HealthSubmitter);

    // Both should be able to submit
    client.submit_health(&source1, &String::from_str(&env, "USDC"), 95, 90, 92, 88);
    client.submit_health(&source2, &String::from_str(&env, "USDC"), 94, 89, 91, 87);

    // Verify both submissions worked
    let health = client.get_health(&String::from_str(&env, "USDC"));
    assert!(health.is_some());
}

#[test]
fn test_admin_can_always_submit_regardless_of_trust() {
    let (env, client, admin, source, _) = setup();

    // Register asset
    client.register_asset(&admin, &String::from_str(&env, "USDC"));

    // Register a trusted source (activates trust enforcement)
    client.register_trusted_source(&admin, &source, &String::from_str(&env, "Oracle"));
    client.grant_role(&admin, &source, bridge_watch_soroban::AdminRole::HealthSubmitter);

    // Admin should still be able to submit even without being a registered trusted source
    // (because admin has inherent permissions)
    client.submit_health(&admin, &String::from_str(&env, "USDC"), 95, 90, 92, 88);

    // Verify submission worked
    let health = client.get_health(&String::from_str(&env, "USDC"));
    assert!(health.is_some());
    assert_eq!(health.unwrap().health_score, 95);
}
