/**
 * Version Migration Helper Tests
 * Comprehensive tests for contract state migration functionality
 */
#[cfg(test)]
mod tests {
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger},
        Address, Env, Map, String as SorobanString, Vec,
    };

    use swipely_contracts::version_migration_helper::{
        EnhancedMigrationHelper, MigrationError, MigrationVersion, ValidationCheckpoint,
    };

    fn v(major: u32, minor: u32, patch: u32) -> MigrationVersion {
        MigrationVersion {
            major,
            minor,
            patch,
        }
    }

    // EnhancedMigrationHelper's functions touch env.storage(), which
    // soroban-sdk only allows from within an active contract call frame, and
    // each require_auth() call consumes that frame's single-use
    // authorization for the given address — so every storage- or
    // auth-touching call below gets its own env.as_contract() block.
    #[contract]
    struct TestContext;

    #[contractimpl]
    impl TestContext {}

    fn setup_env() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(TestContext, ());
        env.ledger().set_timestamp(1_000_000);
        (env, admin, contract_id)
    }

    #[test]
    fn test_initialize_migration() {
        let (env, admin, contract_id) = setup_env();
        let initial_version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        let result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin, initial_version)
        });

        assert!(result.is_ok(), "Migration initialization should succeed");
    }

    #[test]
    fn test_get_version_default() {
        let (env, _, contract_id) = setup_env();

        let version = env.as_contract(&contract_id, || EnhancedMigrationHelper::get_version(&env));

        assert_eq!(version.major, 0);
        assert_eq!(version.minor, 0);
        assert_eq!(version.patch, 0);
    }

    #[test]
    fn test_validate_upgrade_forward() {
        let from = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };
        let to = MigrationVersion {
            major: 1,
            minor: 1,
            patch: 0,
        };

        let result = EnhancedMigrationHelper::validate_upgrade(&from, &to, false);

        assert!(result.is_ok(), "Forward upgrade should be valid");
    }

    #[test]
    fn test_validate_upgrade_same_version() {
        let version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        let result = EnhancedMigrationHelper::validate_upgrade(&version, &version, false);

        assert_eq!(
            result,
            Err(MigrationError::AlreadyAtVersion),
            "Same version upgrade should be rejected"
        );
    }

    #[test]
    fn test_validate_upgrade_downgrade() {
        let from = MigrationVersion {
            major: 2,
            minor: 0,
            patch: 0,
        };
        let to = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        let result = EnhancedMigrationHelper::validate_upgrade(&from, &to, false);

        assert_eq!(
            result,
            Err(MigrationError::VersionDowngradeNotAllowed),
            "Downgrade should be rejected"
        );
    }

    #[test]
    fn test_create_state_snapshot() {
        let (env, admin, contract_id) = setup_env();

        let result = env.as_contract(&contract_id, || {
            let data = Map::new(&env);
            let description = SorobanString::from_str(&env, "Test snapshot");
            EnhancedMigrationHelper::create_state_snapshot(&env, admin, description, data)
        });

        assert!(result.is_ok(), "State snapshot creation should succeed");

        if let Ok(snapshot) = result {
            assert_eq!(snapshot.version.major, 0);
            assert_eq!(snapshot.version.minor, 0);
            assert_eq!(snapshot.version.patch, 0);
            assert!(snapshot.rollback_available);
        }
    }

    #[test]
    fn test_begin_migration() {
        let (env, admin, contract_id) = setup_env();

        let initial_version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), initial_version)
        })
        .expect("Initialize should succeed");

        let target_version = MigrationVersion {
            major: 1,
            minor: 1,
            patch: 0,
        };

        let result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin, target_version, false)
        });

        assert!(result.is_ok(), "Begin migration should succeed");
    }

    #[test]
    fn test_begin_migration_unauthorized() {
        let (env, admin, contract_id) = setup_env();
        let unauthorized = Address::generate(&env);

        let initial_version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin, initial_version)
        })
        .expect("Initialize should succeed");

        let target_version = MigrationVersion {
            major: 1,
            minor: 1,
            patch: 0,
        };

        let result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, unauthorized, target_version, false)
        });

        assert_eq!(
            result,
            Err(MigrationError::UnauthorizedMigrator),
            "Unauthorized user should not be able to begin migration"
        );
    }

    #[test]
    fn test_complete_migration() {
        let (env, admin, contract_id) = setup_env();

        let initial_version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), initial_version.clone())
        })
        .expect("Initialize should succeed");

        let target_version = MigrationVersion {
            major: 1,
            minor: 1,
            patch: 0,
        };

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(
                &env,
                admin.clone(),
                target_version.clone(),
                false,
            )
        })
        .expect("Begin migration should succeed");

        let result = env.as_contract(&contract_id, || {
            let notes = SorobanString::from_str(&env, "Migration notes");
            EnhancedMigrationHelper::complete_migration(
                &env,
                initial_version,
                target_version,
                admin,
                notes,
                false,
            )
        });

        assert!(result.is_ok(), "Complete migration should succeed");

        // Verify version was updated
        let current_version =
            env.as_contract(&contract_id, || EnhancedMigrationHelper::get_version(&env));
        assert_eq!(current_version.major, 1);
        assert_eq!(current_version.minor, 1);
        assert_eq!(current_version.patch, 0);
    }

    #[test]
    fn test_validate_state() {
        let (env, _, contract_id) = setup_env();

        let result = env.as_contract(&contract_id, || {
            let errors = Vec::new(&env);
            let warnings = Vec::new(&env);
            EnhancedMigrationHelper::validate_state(
                &env,
                ValidationCheckpoint::PostMigration,
                errors,
                warnings,
            )
        });

        assert!(result.is_ok(), "State validation should succeed");
    }

    #[test]
    fn test_validate_state_with_errors() {
        let (env, _, contract_id) = setup_env();

        let result = env.as_contract(&contract_id, || {
            let mut errors = Vec::new(&env);
            errors.push_back(SorobanString::from_str(&env, "Test error"));
            let warnings = Vec::new(&env);
            EnhancedMigrationHelper::validate_state(
                &env,
                ValidationCheckpoint::PreMigration,
                errors,
                warnings,
            )
        });

        assert_eq!(
            result,
            Err(MigrationError::ValidationFailed),
            "Validation with errors should fail"
        );
    }

    #[test]
    fn test_get_history() {
        let (env, admin, contract_id) = setup_env();

        let initial_version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin, initial_version)
        })
        .expect("Initialize should succeed");

        let history = env.as_contract(&contract_id, || EnhancedMigrationHelper::get_history(&env));

        assert_eq!(history.len(), 0, "Initial history should be empty");
    }

    #[test]
    fn test_get_snapshots() {
        let (env, _, contract_id) = setup_env();

        let snapshots = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::get_snapshots(&env)
        });

        assert_eq!(snapshots.len(), 0, "Initial snapshots should be empty");
    }

    #[test]
    fn test_add_migrator() {
        let (env, admin, contract_id) = setup_env();
        let new_migrator = Address::generate(&env);

        let initial_version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), initial_version)
        })
        .expect("Initialize should succeed");

        let result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::add_migrator(&env, admin, new_migrator)
        });

        assert!(result.is_ok(), "Adding migrator should succeed");
    }

    #[test]
    fn test_remove_migrator() {
        let (env, admin, contract_id) = setup_env();
        let new_migrator = Address::generate(&env);

        let initial_version = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), initial_version)
        })
        .expect("Initialize should succeed");

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::add_migrator(&env, admin.clone(), new_migrator.clone())
        })
        .expect("Add migrator should succeed");

        let result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::remove_migrator(&env, admin, new_migrator)
        });

        assert!(result.is_ok(), "Removing migrator should succeed");
    }

    #[test]
    fn test_version_comparison_major() {
        let v1 = MigrationVersion {
            major: 1,
            minor: 5,
            patch: 3,
        };
        let v2 = MigrationVersion {
            major: 2,
            minor: 0,
            patch: 0,
        };

        let result = EnhancedMigrationHelper::validate_upgrade(&v1, &v2, false);
        assert!(result.is_ok(), "Major version upgrade should be valid");
    }

    #[test]
    fn test_version_comparison_minor() {
        let v1 = MigrationVersion {
            major: 1,
            minor: 5,
            patch: 3,
        };
        let v2 = MigrationVersion {
            major: 1,
            minor: 6,
            patch: 0,
        };

        let result = EnhancedMigrationHelper::validate_upgrade(&v1, &v2, false);
        assert!(result.is_ok(), "Minor version upgrade should be valid");
    }

    #[test]
    fn test_version_comparison_patch() {
        let v1 = MigrationVersion {
            major: 1,
            minor: 5,
            patch: 3,
        };
        let v2 = MigrationVersion {
            major: 1,
            minor: 5,
            patch: 4,
        };

        let result = EnhancedMigrationHelper::validate_upgrade(&v1, &v2, false);
        assert!(result.is_ok(), "Patch version upgrade should be valid");
    }

    #[test]
    fn test_migration_workflow() {
        let (env, admin, contract_id) = setup_env();

        // Initialize
        let v1 = MigrationVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };
        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v1.clone())
        })
        .expect("Initialize should succeed");

        // Create snapshot
        let snapshot_result = env.as_contract(&contract_id, || {
            let data = Map::new(&env);
            EnhancedMigrationHelper::create_state_snapshot(
                &env,
                admin.clone(),
                SorobanString::from_str(&env, "Pre-migration snapshot"),
                data,
            )
        });
        assert!(snapshot_result.is_ok());

        // Begin migration
        let v2 = MigrationVersion {
            major: 1,
            minor: 1,
            patch: 0,
        };
        let begin_result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin.clone(), v2.clone(), false)
        });
        assert!(begin_result.is_ok());

        // Validate state
        let validate_result = env.as_contract(&contract_id, || {
            let errors = Vec::new(&env);
            let warnings = Vec::new(&env);
            EnhancedMigrationHelper::validate_state(
                &env,
                ValidationCheckpoint::PostMigration,
                errors,
                warnings,
            )
        });
        assert!(validate_result.is_ok());

        // Complete migration
        let complete_result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::complete_migration(
                &env,
                v1,
                v2,
                admin,
                SorobanString::from_str(&env, "Successful migration"),
                false,
            )
        });
        assert!(complete_result.is_ok());

        // Verify final version
        let final_version =
            env.as_contract(&contract_id, || EnhancedMigrationHelper::get_version(&env));
        assert_eq!(final_version.major, 1);
        assert_eq!(final_version.minor, 1);
        assert_eq!(final_version.patch, 0);
    }

    #[test]
    fn test_validate_upgrade_major_skip_rejected() {
        let result = EnhancedMigrationHelper::validate_upgrade(&v(1, 0, 0), &v(3, 0, 0), false);
        assert_eq!(
            result,
            Err(MigrationError::VersionSkipNotAllowed),
            "Skipping an entire major version should be rejected"
        );
    }

    #[test]
    fn test_validate_upgrade_major_skip_allowed_when_forced() {
        let result = EnhancedMigrationHelper::validate_upgrade(&v(1, 0, 0), &v(3, 0, 0), true);
        assert!(result.is_ok(), "A forced major-version skip should succeed");
    }

    #[test]
    fn test_validate_upgrade_downgrade_allowed_when_forced() {
        let result = EnhancedMigrationHelper::validate_upgrade(&v(2, 0, 0), &v(1, 0, 0), true);
        assert!(result.is_ok(), "A forced downgrade should succeed");
    }

    #[test]
    fn test_begin_migration_rejects_version_skip_unless_forced() {
        let (env, admin, contract_id) = setup_env();

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v(1, 0, 0))
        })
        .expect("Initialize should succeed");

        let unforced = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin.clone(), v(3, 0, 0), false)
        });
        assert_eq!(
            unforced,
            Err(MigrationError::VersionSkipNotAllowed),
            "begin_migration should reject a major-version skip by default"
        );

        let forced = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin, v(3, 0, 0), true)
        });
        assert!(
            forced.is_ok(),
            "begin_migration should allow a forced major-version skip"
        );
    }

    #[test]
    fn test_begin_migration_rejects_downgrade_unless_forced() {
        let (env, admin, contract_id) = setup_env();

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v(2, 0, 0))
        })
        .expect("Initialize should succeed");

        let unforced = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin.clone(), v(1, 0, 0), false)
        });
        assert_eq!(
            unforced,
            Err(MigrationError::VersionDowngradeNotAllowed),
            "begin_migration should reject a downgrade by default"
        );

        let forced = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin, v(1, 0, 0), true)
        });
        assert!(
            forced.is_ok(),
            "begin_migration should allow a forced downgrade"
        );
    }

    #[test]
    fn test_dry_run_migration_valid_target_does_not_mutate_state() {
        let (env, admin, contract_id) = setup_env();

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v(1, 0, 0))
        })
        .expect("Initialize should succeed");

        let report = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::dry_run_migration(
                &env,
                admin,
                v(1, 1, 0),
                Vec::new(&env),
                Vec::new(&env),
                false,
            )
        });

        assert!(report.would_succeed, "a valid target should dry-run clean");
        assert_eq!(report.issues.len(), 0);
        assert_eq!(report.current_version, v(1, 0, 0));
        assert_eq!(report.target_version, v(1, 1, 0));

        // A dry run must not mutate any state: version, history, and the
        // in-progress flag should all be untouched.
        let version_after =
            env.as_contract(&contract_id, || EnhancedMigrationHelper::get_version(&env));
        assert_eq!(version_after, v(1, 0, 0));

        let history_after =
            env.as_contract(&contract_id, || EnhancedMigrationHelper::get_history(&env));
        assert_eq!(history_after.len(), 0);

        // begin_migration must still be able to proceed afterwards, proving
        // the dry run didn't set the mutual-exclusion flag.
        let begin_result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(
                &env,
                Address::generate(&env),
                v(1, 1, 0),
                false,
            )
        });
        // The fresh address isn't an authorized migrator, but the error
        // proves the call reached the authorization check rather than
        // failing on a stale "migration in progress" flag left by dry_run.
        assert_eq!(begin_result, Err(MigrationError::UnauthorizedMigrator));
    }

    #[test]
    fn test_dry_run_migration_reports_rejected_target_without_force() {
        let (env, admin, contract_id) = setup_env();

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v(1, 0, 0))
        })
        .expect("Initialize should succeed");

        let report = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::dry_run_migration(
                &env,
                admin,
                v(3, 0, 0),
                Vec::new(&env),
                Vec::new(&env),
                false,
            )
        });

        assert!(
            !report.would_succeed,
            "a version-skipping target should be reported as failing"
        );
        assert_eq!(report.issues.len(), 1);
    }

    #[test]
    fn test_dry_run_migration_reports_storage_errors_and_unauthorized_caller() {
        let (env, admin, contract_id) = setup_env();
        let stranger = Address::generate(&env);

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin, v(1, 0, 0))
        })
        .expect("Initialize should succeed");

        let mut storage_errors = Vec::new(&env);
        storage_errors.push_back(SorobanString::from_str(&env, "missing storage key: foo"));

        let report = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::dry_run_migration(
                &env,
                stranger,
                v(1, 1, 0),
                storage_errors,
                Vec::new(&env),
                false,
            )
        });

        assert!(!report.would_succeed);
        // One issue for the unauthorized caller, one for the storage check.
        assert_eq!(report.issues.len(), 2);
    }

    #[test]
    fn test_cancel_migration_clears_stuck_in_progress_flag() {
        let (env, admin, contract_id) = setup_env();

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v(1, 0, 0))
        })
        .expect("Initialize should succeed");

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin.clone(), v(1, 1, 0), false)
        })
        .expect("Begin migration should succeed");

        // Simulate the off-chain migration failing before complete_migration
        // is ever called: begin_migration again should now be blocked.
        let blocked = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin.clone(), v(1, 1, 0), false)
        });
        assert_eq!(blocked, Err(MigrationError::MigrationInProgress));

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::cancel_migration(&env, admin.clone())
        })
        .expect("Cancelling a stuck migration should succeed");

        // A fresh begin_migration should now proceed, and the version is
        // unchanged since the migration never completed.
        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin, v(1, 1, 0), false)
        })
        .expect("begin_migration should succeed again after cancel_migration");

        let version = env.as_contract(&contract_id, || EnhancedMigrationHelper::get_version(&env));
        assert_eq!(version, v(1, 0, 0));
    }

    #[test]
    fn test_cancel_migration_fails_when_nothing_in_progress() {
        let (env, admin, contract_id) = setup_env();

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v(1, 0, 0))
        })
        .expect("Initialize should succeed");

        let result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::cancel_migration(&env, admin)
        });

        assert_eq!(result, Err(MigrationError::NoMigrationInProgress));
    }

    #[test]
    fn test_cancel_migration_unauthorized() {
        let (env, admin, contract_id) = setup_env();
        let stranger = Address::generate(&env);

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::initialize(&env, admin.clone(), v(1, 0, 0))
        })
        .expect("Initialize should succeed");

        env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::begin_migration(&env, admin, v(1, 1, 0), false)
        })
        .expect("Begin migration should succeed");

        let result = env.as_contract(&contract_id, || {
            EnhancedMigrationHelper::cancel_migration(&env, stranger)
        });

        assert_eq!(result, Err(MigrationError::UnauthorizedMigrator));
    }
}
