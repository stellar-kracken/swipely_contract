//! Global submission pause controls for Bridge Watch.
//!
//! Allows admins to pause and resume data submissions contract-wide while
//! keeping all read-only query entrypoints available.

use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

use crate::keys;

/// Maximum pause history entries retained on-chain.
pub const MAX_PAUSE_HISTORY: u32 = 50;

/// On-chain submission pause state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmissionPauseState {
    pub paused: bool,
    pub reason: String,
    pub paused_by: Address,
    pub paused_at: u64,
    pub resumed_at: u64,
}

/// One entry in the pause/unpause audit trail.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseHistoryEntry {
    pub paused: bool,
    pub reason: String,
    pub actor: Address,
    pub timestamp: u64,
}

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&keys::ADMIN)
        .unwrap_or_else(|| panic!("contract not initialized"));
    if *caller != admin {
        panic!("only admin can manage submission pause");
    }
}

fn load_history(env: &Env) -> Vec<PauseHistoryEntry> {
    env.storage()
        .instance()
        .get(&keys::PAUSE_HISTORY)
        .unwrap_or_else(|| Vec::new(env))
}

fn append_history(env: &Env, entry: PauseHistoryEntry) {
    let mut history = load_history(env);
    history.push_back(entry);
    if history.len() > MAX_PAUSE_HISTORY {
        let mut trimmed: Vec<PauseHistoryEntry> = Vec::new(env);
        for i in 1..history.len() {
            trimmed.push_back(history.get(i).unwrap());
        }
        history = trimmed;
    }
    env.storage()
        .instance()
        .set(&keys::PAUSE_HISTORY, &history);
}

/// Returns `true` when contract-wide data submissions are paused.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&keys::GLOBAL_PAUSED)
        .unwrap_or(false)
}

/// Panics when submissions are globally paused.
pub fn assert_not_paused(env: &Env) {
    if is_paused(env) {
        panic!("data submissions are globally paused");
    }
}

/// Read the current submission pause state.
pub fn get_state(env: &Env) -> SubmissionPauseState {
    let paused = is_paused(env);
    let reason: String = env
        .storage()
        .instance()
        .get(&keys::PAUSE_REASON)
        .unwrap_or_else(|| String::from_str(env, ""));
    let paused_by: Address = env
        .storage()
        .instance()
        .get(&keys::PAUSE_GUARDIAN)
        .unwrap_or_else(|| {
            env.storage()
                .instance()
                .get(&keys::ADMIN)
                .unwrap_or_else(|| panic!("contract not initialized"))
        });
    let paused_at: u64 = env
        .storage()
        .instance()
        .get(&keys::PAUSED_AT)
        .unwrap_or(0);
    let resumed_at: u64 = env
        .storage()
        .instance()
        .get(&keys::UNPAUSE_AVAILABLE_AT)
        .unwrap_or(0);

    SubmissionPauseState {
        paused,
        reason,
        paused_by,
        paused_at,
        resumed_at,
    }
}

/// Pause all mutating data submissions. Admin only.
pub fn pause(env: Env, caller: Address, reason: String) {
    require_admin(&env, &caller);
    if is_paused(&env) {
        panic!("submissions are already paused");
    }
    if reason.len() == 0 {
        panic!("pause reason must not be empty");
    }

    let now = env.ledger().timestamp();
    env.storage().instance().set(&keys::GLOBAL_PAUSED, &true);
    env.storage()
        .instance()
        .set(&keys::PAUSE_REASON, &reason);
    env.storage()
        .instance()
        .set(&keys::PAUSE_GUARDIAN, &caller);
    env.storage().instance().set(&keys::PAUSED_AT, &now);

    append_history(
        &env,
        PauseHistoryEntry {
            paused: true,
            reason: reason.clone(),
            actor: caller.clone(),
            timestamp: now,
        },
    );

    env.events()
        .publish((symbol_short!("sub_pau"), caller), (reason, now));
}

/// Resume data submissions. Admin only.
pub fn resume(env: Env, caller: Address) {
    require_admin(&env, &caller);
    if !is_paused(&env) {
        panic!("submissions are not paused");
    }

    let now = env.ledger().timestamp();
    env.storage().instance().set(&keys::GLOBAL_PAUSED, &false);
    env.storage()
        .instance()
        .set(&keys::UNPAUSE_AVAILABLE_AT, &now);

    append_history(
        &env,
        PauseHistoryEntry {
            paused: false,
            reason: String::from_str(&env, "submissions resumed"),
            actor: caller.clone(),
            timestamp: now,
        },
    );

    env.events()
        .publish((symbol_short!("sub_res"), caller), now);
}

/// Return pause/unpause history (most recent last).
pub fn get_history(env: Env) -> Vec<PauseHistoryEntry> {
    load_history(&env)
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
    fn test_pause_and_resume_submissions() {
        let (env, admin) = setup();
        assert!(!is_paused(&env));

        pause(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "maintenance window"),
        );
        assert!(is_paused(&env));

        let state = get_state(&env);
        assert!(state.paused);
        assert_eq!(state.reason, String::from_str(&env, "maintenance window"));

        resume(env.clone(), admin.clone());
        assert!(!is_paused(&env));

        let history = get_history(env);
        assert_eq!(history.len(), 2);
    }

    #[test]
    #[should_panic(expected = "already paused")]
    fn test_double_pause_fails() {
        let (env, admin) = setup();
        pause(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "first"),
        );
        pause(env, admin, String::from_str(&env, "second"));
    }

    #[test]
    #[should_panic(expected = "not paused")]
    fn test_resume_when_not_paused_fails() {
        let (env, admin) = setup();
        resume(env, admin);
    }

    #[test]
    #[should_panic(expected = "globally paused")]
    fn test_assert_not_paused_panics() {
        let (env, admin) = setup();
        pause(
            env.clone(),
            admin,
            String::from_str(&env, "halt"),
        );
        assert_not_paused(&env);
    }
}
