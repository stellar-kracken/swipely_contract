/**
 * Emergency Fund Recovery Tests
 * Comprehensive tests for the emergency fund recovery functionality
 */

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger},
        Address, Env, String,
    };

    use swipely_contracts::emergency_fund_recovery::{
        EmergencyFundRecovery, RecoveryError,
    };

    // EmergencyFundRecovery's functions touch env.storage(), which soroban-sdk
    // only allows from within an active contract call frame, and each
    // require_auth() call consumes that frame's single-use authorization for
    // the given address — so every call below gets its own env.as_contract()
    // block.
    #[contract]
    struct TestContext;

    #[contractimpl]
    impl TestContext {}

    // Helper function to setup test environment
    fn setup_env() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let contract_id = env.register_contract(None, TestContext);

        // Mock ledger setup
        env.ledger().set_timestamp(1_000_000);

        (env, admin, user, contract_id)
    }

    #[test]
    fn test_initialize_recovery() {
        let (env, admin, _, contract_id) = setup_env();

        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(
                env.clone(),
                admin,
                172_800, // 48 hours
            )
        });

        assert!(result.is_ok(), "Recovery initialization should succeed");
    }

    #[test]
    fn test_initialize_recovery_invalid_timelock_zero() {
        let (env, admin, _, contract_id) = setup_env();

        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin, 0)
        });

        assert_eq!(
            result,
            Err(RecoveryError::InvalidTimelock),
            "Should reject zero timelock"
        );
    }

    #[test]
    fn test_initialize_recovery_invalid_timelock_too_large() {
        let (env, admin, _, contract_id) = setup_env();

        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(
                env.clone(),
                admin,
                31_536_001, // More than 1 year
            )
        });

        assert_eq!(
            result,
            Err(RecoveryError::InvalidTimelock),
            "Should reject timelock larger than 1 year"
        );
    }

    #[test]
    fn test_enable_emergency_recovery() {
        let (env, admin, _, contract_id) = setup_env();

        // Initialize first
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");

        // Enable recovery
        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), admin)
        });

        assert!(result.is_ok(), "Enable emergency recovery should succeed");
    }

    #[test]
    fn test_enable_emergency_recovery_unauthorized() {
        let (env, admin, user, contract_id) = setup_env();

        // Initialize with admin
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin, 172_800)
        })
        .expect("Initialization should succeed");

        // Try to enable with non-admin
        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), user)
        });

        assert_eq!(
            result,
            Err(RecoveryError::NotAuthorized),
            "Non-admin should not be able to enable recovery"
        );
    }

    #[test]
    fn test_add_recovery_authorizer() {
        let (env, admin, user, contract_id) = setup_env();

        // Initialize
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");

        // Add authorizer
        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::add_recovery_authorizer(
                env.clone(),
                admin,
                user,
                true, // can_initiate
                true, // can_approve
                true, // can_execute
                true, // can_cancel
            )
        });

        assert!(result.is_ok(), "Adding recovery authorizer should succeed");
    }

    #[test]
    fn test_initiate_recovery_without_emergency_enabled() {
        let (env, admin, user, contract_id) = setup_env();

        // Initialize
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin, 172_800)
        })
        .expect("Initialization should succeed");

        // Try to initiate recovery without enabling emergency mode
        let destination = Address::generate(&env);
        let token_address = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initiate_recovery(
                env.clone(),
                user,
                destination,
                token_address,
                1000,
                String::from_slice(&env, "test recovery"),
            )
        });

        assert_eq!(
            result,
            Err(RecoveryError::EmergencyModeDisabled),
            "Should not allow recovery when emergency mode is disabled"
        );
    }

    #[test]
    fn test_initiate_recovery_invalid_amount() {
        let (env, admin, user, contract_id) = setup_env();

        // Initialize and enable
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), admin.clone())
        })
        .expect("Enable should succeed");

        // Add user as authorizer
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::add_recovery_authorizer(
                env.clone(),
                admin,
                user.clone(),
                true,
                true,
                true,
                true,
            )
        })
        .expect("Add authorizer should succeed");

        // Try to initiate with invalid amount
        let destination = Address::generate(&env);
        let token_address = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initiate_recovery(
                env.clone(),
                user,
                destination,
                token_address,
                0, // Invalid amount
                String::from_slice(&env, "test recovery"),
            )
        });

        assert_eq!(
            result,
            Err(RecoveryError::InvalidAmount),
            "Should reject zero or negative amount"
        );
    }

    #[test]
    fn test_initiate_recovery_same_destination() {
        let (env, admin, user, contract_id) = setup_env();

        // Setup
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), admin.clone())
        })
        .expect("Enable should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::add_recovery_authorizer(
                env.clone(),
                admin,
                user.clone(),
                true,
                true,
                true,
                true,
            )
        })
        .expect("Add authorizer should succeed");

        let token_address = Address::generate(&env);

        // Try to initiate with same destination and initiator
        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initiate_recovery(
                env.clone(),
                user.clone(),
                user, // Same as initiator
                token_address,
                1000,
                String::from_slice(&env, "test recovery"),
            )
        });

        assert_eq!(
            result,
            Err(RecoveryError::InvalidDestination),
            "Should not allow same destination as initiator"
        );
    }

    #[test]
    fn test_initiate_recovery_success() {
        let (env, admin, user, contract_id) = setup_env();

        // Setup
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), admin.clone())
        })
        .expect("Enable should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::add_recovery_authorizer(
                env.clone(),
                admin,
                user.clone(),
                true,
                true,
                true,
                true,
            )
        })
        .expect("Add authorizer should succeed");

        let destination = Address::generate(&env);
        let token_address = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initiate_recovery(
                env.clone(),
                user,
                destination,
                token_address,
                1000,
                String::from_slice(&env, "emergency recovery"),
            )
        });

        assert!(result.is_ok(), "Recovery initiation should succeed");

        if let Ok(recovery_id) = result {
            assert_eq!(recovery_id, 1, "First recovery should have ID 1");
        }
    }

    #[test]
    fn test_approve_recovery() {
        let (env, admin, user, contract_id) = setup_env();

        // Setup
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), admin.clone())
        })
        .expect("Enable should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::add_recovery_authorizer(
                env.clone(),
                admin.clone(),
                user.clone(),
                true,
                true,
                true,
                true,
            )
        })
        .expect("Add authorizer should succeed");

        // Initiate recovery
        let destination = Address::generate(&env);
        let token_address = Address::generate(&env);
        let recovery_id = env
            .as_contract(&contract_id, || {
                EmergencyFundRecovery::initiate_recovery(
                    env.clone(),
                    user.clone(),
                    destination,
                    token_address,
                    1000,
                    String::from_slice(&env, "emergency recovery"),
                )
            })
            .expect("Recovery initiation should succeed");

        // Approve recovery
        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::approve_recovery(env.clone(), user, recovery_id)
        });

        assert!(result.is_ok(), "Recovery approval should succeed");
    }

    #[test]
    fn test_cancel_recovery() {
        let (env, admin, user, contract_id) = setup_env();

        // Setup
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), admin.clone())
        })
        .expect("Enable should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::add_recovery_authorizer(
                env.clone(),
                admin.clone(),
                user.clone(),
                true,
                true,
                true,
                true,
            )
        })
        .expect("Add authorizer should succeed");

        // Initiate recovery
        let destination = Address::generate(&env);
        let token_address = Address::generate(&env);
        let recovery_id = env
            .as_contract(&contract_id, || {
                EmergencyFundRecovery::initiate_recovery(
                    env.clone(),
                    user.clone(),
                    destination,
                    token_address,
                    1000,
                    String::from_slice(&env, "emergency recovery"),
                )
            })
            .expect("Recovery initiation should succeed");

        // Cancel recovery
        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::cancel_recovery(
                env.clone(),
                user,
                recovery_id,
                String::from_slice(&env, "cancelled"),
            )
        });

        assert!(result.is_ok(), "Recovery cancellation should succeed");
    }

    #[test]
    fn test_get_recovery() {
        let (env, admin, user, contract_id) = setup_env();

        // Setup
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin.clone(), 172_800)
        })
        .expect("Initialization should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::enable_emergency_recovery(env.clone(), admin.clone())
        })
        .expect("Enable should succeed");
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::add_recovery_authorizer(
                env.clone(),
                admin,
                user.clone(),
                true,
                true,
                true,
                true,
            )
        })
        .expect("Add authorizer should succeed");

        // Initiate recovery
        let destination = Address::generate(&env);
        let token_address = Address::generate(&env);
        let recovery_id = env
            .as_contract(&contract_id, || {
                EmergencyFundRecovery::initiate_recovery(
                    env.clone(),
                    user,
                    destination.clone(),
                    token_address.clone(),
                    1000,
                    String::from_slice(&env, "emergency recovery"),
                )
            })
            .expect("Recovery initiation should succeed");

        // Get recovery
        let result = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::get_recovery(env.clone(), recovery_id)
        });

        assert!(result.is_ok(), "Get recovery should succeed");

        if let Ok(recovery) = result {
            assert_eq!(recovery.recovery_id, recovery_id);
            assert_eq!(recovery.amount, 1000);
            assert_eq!(recovery.destination, destination);
        }
    }

    #[test]
    fn test_get_total_recovered() {
        let (env, admin, _, contract_id) = setup_env();

        // Initialize
        env.as_contract(&contract_id, || {
            EmergencyFundRecovery::initialize_recovery(env.clone(), admin, 172_800)
        })
        .expect("Initialization should succeed");

        let total = env.as_contract(&contract_id, || {
            EmergencyFundRecovery::get_total_recovered(env.clone())
        });

        assert_eq!(total, 0, "Initial total recovered should be 0");
    }
}
