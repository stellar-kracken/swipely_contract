#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec,
};

/// Maximum transition log entries retained per transfer (gas-bounded audit trail).
const MAX_HISTORY_PER_TRANSFER: u32 = 48;

const EVT_INIT: Symbol = symbol_short!("tr_init");
const EVT_ADV: Symbol = symbol_short!("tr_adv");
const EVT_TO: Symbol = symbol_short!("tr_to");
const EVT_RB: Symbol = symbol_short!("tr_rb");
const EVT_ADM: Symbol = symbol_short!("tr_adm");
const EVT_VRF: Symbol = symbol_short!("tr_vrf");
const EVT_ORC: Symbol = symbol_short!("tr_orc");
const EVT_AUTO: Symbol = symbol_short!("tr_aut");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// High-level bridge technology (affects off-chain handling; stored for routing).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BridgeType {
    LockMint,
    BurnRelease,
    NativeWrapped,
    Cctp,
    Custom,
}

/// Operational mode (standard vs expedited vs insured settlement path).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferMode {
    Standard,
    FastTrack,
    Insured,
}

/// Lifecycle states for a single bridge transfer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferState {
    /// Transfer record created; awaiting source-chain acknowledgement.
    Initiated,
    /// Waiting for source finality / attestations.
    AwaitingSource,
    /// Funds should move into escrow on the locking side.
    EscrowPending,
    /// Escrow has locked liquidity for this transfer.
    EscrowLocked,
    /// Reserve / proof verification (integrates [`BridgeReserveVerifier`]-style checks off-chain + callback).
    VerificationPending,
    /// Oracle or price-feed attestation step.
    OraclePending,
    /// Ready to release on destination; final checks.
    ReleasePending,
    /// Terminal: success.
    Completed,
    /// Terminal: unrecoverable failure.
    Failed,
    /// Intermediate rollback in progress.
    RollingBack,
    /// Terminal: funds returned / burn reversed per bridge policy.
    RolledBack,
    /// Terminal: stuck beyond configured deadline without progress.
    TimedOut,
}

/// On-chain transfer record (persistent).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeTransfer {
    pub id: u64,
    pub initiator: Address,
    pub bridge_type: BridgeType,
    pub mode: TransferMode,
    pub asset: String,
    pub amount: i128,
    /// Opaque destination reference (chain id, address, memo, etc.).
    pub dest_hint: String,
    pub state: TransferState,
    /// Ledger timestamp by which the current state should advance or time out.
    pub state_deadline: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub verification_ok: bool,
    pub oracle_ok: bool,
}

/// One append-only history row (recovery & analytics).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateTransitionLog {
    pub from_state: TransferState,
    pub to_state: TransferState,
    pub timestamp: u64,
    pub actor: Address,
    pub note: String,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    TransferSeq,
    Transfer(u64),
    History(u64),
    Timeout(TransferState),
    DefaultTimeoutSecs,
    EscrowContract,
    VerifierContract,
    OracleContract,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct TransferStateMachine;

#[contractimpl]
impl TransferStateMachine {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::TransferSeq, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::DefaultTimeoutSecs, &86_400u64);
        // Sensible defaults per state (seconds). All configurable via `set_state_timeout`.
        Self::seed_default_timeouts(&env);
        // Placeholder zero addresses until `set_integration_contracts`.
        let zero = admin.clone(); // valid Address; overwritten by admin
        env.storage()
            .instance()
            .set(&DataKey::EscrowContract, &zero);
        env.storage()
            .instance()
            .set(&DataKey::VerifierContract, &zero);
        env.storage()
            .instance()
            .set(&DataKey::OracleContract, &zero);
    }

    /// Wire escrow, verification, and oracle contracts for auth-gated automated steps.
    pub fn set_integration_contracts(
        env: Env,
        caller: Address,
        escrow: Address,
        verifier: Address,
        oracle: Address,
    ) {
        Self::assert_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&DataKey::EscrowContract, &escrow);
        env.storage()
            .instance()
            .set(&DataKey::VerifierContract, &verifier);
        env.storage()
            .instance()
            .set(&DataKey::OracleContract, &oracle);
    }

    pub fn set_default_timeout(env: Env, caller: Address, secs: u64) {
        Self::assert_admin(&env, &caller);
        if secs == 0 {
            panic!("timeout must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::DefaultTimeoutSecs, &secs);
    }

    /// Per-state timeout override (seconds spent in `state` before `handle_timeout` may fire).
    pub fn set_state_timeout(env: Env, caller: Address, state: TransferState, secs: u64) {
        Self::assert_admin(&env, &caller);
        if secs == 0 {
            panic!("timeout must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::Timeout(state), &secs);
    }

    /// Start a new transfer; initial state is always [`TransferState::Initiated`].
    pub fn initiate_transfer(
        env: Env,
        initiator: Address,
        bridge_type: BridgeType,
        mode: TransferMode,
        asset: String,
        amount: i128,
        dest_hint: String,
    ) -> u64 {
        initiator.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let seq: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TransferSeq)
            .expect("not initialized");
        let id = seq + 1;
        env.storage().instance().set(&DataKey::TransferSeq, &id);

        let now = env.ledger().timestamp();
        let deadline = now + Self::timeout_secs_for(&env, &TransferState::Initiated);

        let t = BridgeTransfer {
            id,
            initiator: initiator.clone(),
            bridge_type,
            mode,
            asset,
            amount,
            dest_hint,
            state: TransferState::Initiated,
            state_deadline: deadline,
            created_at: now,
            updated_at: now,
            verification_ok: false,
            oracle_ok: false,
        };

        env.storage().persistent().set(&DataKey::Transfer(id), &t);
        let empty: Vec<StateTransitionLog> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::History(id), &empty);

        env.events().publish((EVT_INIT, id), t.state.clone());

        id
    }

    /// Returns whether `from_state` → `to_state` is allowed for `transfer_id`'s current mode
    /// (read-only; does not mutate). If transfer missing, returns false.
    pub fn verify_transition(
        env: Env,
        transfer_id: u64,
        from_state: TransferState,
        to_state: TransferState,
    ) -> bool {
        let t: Option<BridgeTransfer> = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id));
        let Some(tr) = t else {
            return false;
        };
        if tr.state != from_state {
            return false;
        }
        Self::transition_allowed(&tr, &from_state, &to_state)
    }

    /// Perform a valid state transition (enforces caller authorization per edge).
    pub fn advance_state(env: Env, caller: Address, transfer_id: u64, next: TransferState) {
        caller.require_auth();
        let mut t: BridgeTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .expect("unknown transfer");
        let from = t.state.clone();
        if !Self::transition_allowed(&t, &from, &next) {
            panic!("invalid transition");
        }
        Self::assert_advance_auth(&env, &caller, &t, &from, &next);

        Self::apply_transition(
            &env,
            &mut t,
            &caller,
            &from,
            &next,
            String::from_str(&env, ""),
        );
    }

    /// If the current state's deadline has passed, move to [`TransferState::TimedOut`] (terminal).
    pub fn handle_timeout(env: Env, transfer_id: u64) {
        let mut t: BridgeTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .expect("unknown transfer");
        if Self::is_terminal(&t.state) {
            return;
        }
        let now = env.ledger().timestamp();
        if now <= t.state_deadline {
            panic!("timeout not reached");
        }
        let from = t.state.clone();
        if !Self::transition_allowed(&t, &from, &TransferState::TimedOut) {
            panic!("cannot timeout from this state");
        }
        // Anyone may observe and record timeout (permissionless liveness).
        let system = t.initiator.clone();
        Self::apply_transition(
            &env,
            &mut t,
            &system,
            &from,
            &TransferState::TimedOut,
            String::from_str(&env, "timeout"),
        );
        env.events().publish((EVT_TO, transfer_id), ());
    }

    /// Roll back an in-flight transfer (initiator or admin). Two-step collapsed: RollingBack → RolledBack.
    pub fn rollback_transfer(env: Env, caller: Address, transfer_id: u64, reason: String) {
        caller.require_auth();
        let mut t: BridgeTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .expect("unknown transfer");
        if !(caller == t.initiator || Self::is_admin(&env, &caller)) {
            panic!("unauthorized rollback");
        }
        if Self::is_terminal(&t.state) {
            panic!("cannot rollback terminal transfer");
        }
        let from = t.state.clone();
        if !Self::can_rollback_from(&from) {
            panic!("rollback not allowed from this state");
        }

        Self::apply_transition(
            &env,
            &mut t,
            &caller,
            &from,
            &TransferState::RollingBack,
            reason.clone(),
        );
        let from2 = t.state.clone();
        Self::apply_transition(
            &env,
            &mut t,
            &caller,
            &from2,
            &TransferState::RolledBack,
            reason,
        );
        env.events().publish((EVT_RB, transfer_id), ());
    }

    /// Admin recovery: force a **legal** next state (e.g. unstick after incident review).
    pub fn admin_override_state(
        env: Env,
        admin: Address,
        transfer_id: u64,
        next: TransferState,
        note: String,
    ) {
        Self::assert_admin(&env, &admin);
        let mut t: BridgeTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .expect("unknown transfer");
        let from = t.state.clone();
        // `TimedOut` is terminal but admin may recover; true finals cannot be overridden.
        if matches!(
            &from,
            TransferState::Completed | TransferState::Failed | TransferState::RolledBack
        ) {
            panic!("cannot override terminal state");
        }
        if !Self::admin_override_allowed(&from, &next) {
            panic!("override transition not permitted");
        }
        Self::apply_transition(&env, &mut t, &admin, &from, &next, note);
        env.events().publish((EVT_ADM, transfer_id), next);
    }

    /// Called by the configured verification contract to record pass/fail.
    pub fn submit_verification_result(env: Env, caller: Address, transfer_id: u64, ok: bool) {
        caller.require_auth();
        let verifier: Address = env
            .storage()
            .instance()
            .get(&DataKey::VerifierContract)
            .expect("not initialized");
        if caller != verifier {
            panic!("only verifier contract");
        }
        let mut t: BridgeTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .expect("unknown transfer");
        if t.state != TransferState::VerificationPending {
            panic!("wrong state for verification");
        }
        t.verification_ok = ok;
        t.updated_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::Transfer(transfer_id), &t);
        env.events().publish((EVT_VRF, transfer_id), ok);
    }

    /// Called by the configured oracle contract.
    pub fn submit_oracle_result(env: Env, caller: Address, transfer_id: u64, ok: bool) {
        caller.require_auth();
        let oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::OracleContract)
            .expect("not initialized");
        if caller != oracle {
            panic!("only oracle contract");
        }
        let mut t: BridgeTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .expect("unknown transfer");
        if t.state != TransferState::OraclePending {
            panic!("wrong state for oracle");
        }
        t.oracle_ok = ok;
        t.updated_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::Transfer(transfer_id), &t);
        env.events().publish((EVT_ORC, transfer_id), ok);
    }

    /// Permissionless hook: advances when preconditions are satisfied (verified + oracle flags).
    pub fn try_auto_progress(env: Env, transfer_id: u64) {
        let mut t: BridgeTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
            .expect("unknown transfer");
        if Self::is_terminal(&t.state) {
            return;
        }

        let from = t.state.clone();
        let maybe_next: Option<TransferState> = match t.state {
            TransferState::VerificationPending if t.verification_ok => {
                Some(TransferState::OraclePending)
            }
            TransferState::OraclePending if t.oracle_ok => Some(TransferState::ReleasePending),
            _ => None,
        };

        if let Some(next) = maybe_next {
            if Self::transition_allowed(&t, &from, &next) {
                // Automated progression uses initiator as logical actor (zero-sig path).
                let actor = t.initiator.clone();
                Self::apply_transition(
                    &env,
                    &mut t,
                    &actor,
                    &from,
                    &next,
                    String::from_str(&env, "auto"),
                );
                env.events().publish((EVT_AUTO, transfer_id), next);
            }
        }
    }

    pub fn get_transfer(env: Env, transfer_id: u64) -> Option<BridgeTransfer> {
        env.storage()
            .persistent()
            .get(&DataKey::Transfer(transfer_id))
    }

    pub fn get_transition_history(env: Env, transfer_id: u64) -> Vec<StateTransitionLog> {
        env.storage()
            .persistent()
            .get(&DataKey::History(transfer_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_timeout_for_state(env: Env, state: TransferState) -> u64 {
        Self::timeout_secs_for(&env, &state)
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn seed_default_timeouts(env: &Env) {
        use TransferState::*;
        let pairs: [(TransferState, u64); 9] = [
            (Initiated, 3_600),
            (AwaitingSource, 86_400),
            (EscrowPending, 86_400),
            (EscrowLocked, 172_800),
            (VerificationPending, 43_200),
            (OraclePending, 7_200),
            (ReleasePending, 86_400),
            (RollingBack, 86_400),
            (Failed, 60),
        ];
        for (s, secs) in pairs.iter() {
            env.storage()
                .instance()
                .set(&DataKey::Timeout(s.clone()), secs);
        }
    }

    fn timeout_secs_for(env: &Env, state: &TransferState) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::Timeout(state.clone()))
            .unwrap_or_else(|| {
                env.storage()
                    .instance()
                    .get(&DataKey::DefaultTimeoutSecs)
                    .unwrap_or(86_400)
            })
    }

    fn is_terminal(s: &TransferState) -> bool {
        use TransferState::*;
        matches!(s, Completed | Failed | RolledBack | TimedOut)
    }

    fn can_rollback_from(s: &TransferState) -> bool {
        use TransferState::*;
        matches!(
            s,
            Initiated
                | AwaitingSource
                | EscrowPending
                | EscrowLocked
                | VerificationPending
                | OraclePending
                | ReleasePending
        )
    }

    fn transition_allowed(t: &BridgeTransfer, from: &TransferState, to: &TransferState) -> bool {
        use TransferState::*;
        if from == to {
            return false;
        }
        match (from, to) {
            // Happy path
            (&Initiated, &AwaitingSource) => true,
            (&Initiated, &EscrowPending) => t.mode == TransferMode::FastTrack,
            (&AwaitingSource, &EscrowPending) => true,
            (&EscrowPending, &EscrowLocked) => true,
            (&EscrowLocked, &VerificationPending) => true,
            (&VerificationPending, &OraclePending) => t.verification_ok,
            (&OraclePending, &ReleasePending) => t.oracle_ok,
            (&ReleasePending, &Completed) => true,

            // Failure
            (&Initiated, &Failed) => true,
            (&AwaitingSource, &Failed) => true,
            (&EscrowPending, &Failed) => true,
            (&EscrowLocked, &Failed) => true,
            (&VerificationPending, &Failed) => true,
            (&OraclePending, &Failed) => true,
            (&ReleasePending, &Failed) => true,

            // Timeout from any non-terminal except RollingBack intermediate
            (&Initiated, &TimedOut) => true,
            (&AwaitingSource, &TimedOut) => true,
            (&EscrowPending, &TimedOut) => true,
            (&EscrowLocked, &TimedOut) => true,
            (&VerificationPending, &TimedOut) => true,
            (&OraclePending, &TimedOut) => true,
            (&ReleasePending, &TimedOut) => true,

            // Rollback entry
            (&Initiated, &RollingBack) => true,
            (&AwaitingSource, &RollingBack) => true,
            (&EscrowPending, &RollingBack) => true,
            (&EscrowLocked, &RollingBack) => true,
            (&VerificationPending, &RollingBack) => true,
            (&OraclePending, &RollingBack) => true,
            (&ReleasePending, &RollingBack) => true,

            (&RollingBack, &RolledBack) => true,

            _ => false,
        }
    }

    fn admin_override_allowed(from: &TransferState, to: &TransferState) -> bool {
        use TransferState::*;
        matches!(
            (from, to),
            (&TimedOut, &Failed)
                | (&TimedOut, &RolledBack)
                | (&TimedOut, &EscrowLocked)
                | (&TimedOut, &ReleasePending)
                | (&Failed, &RollingBack)
                | (&OraclePending, &ReleasePending)
                | (&VerificationPending, &OraclePending)
        )
    }

    fn assert_advance_auth(
        env: &Env,
        caller: &Address,
        t: &BridgeTransfer,
        from: &TransferState,
        to: &TransferState,
    ) {
        use TransferState::*;
        let escrow: Address = env
            .storage()
            .instance()
            .get(&DataKey::EscrowContract)
            .unwrap();
        let admin_ok = Self::is_admin(env, caller);
        let initiator_ok = *caller == t.initiator;

        match (from, to) {
            (&EscrowPending, &EscrowLocked) => {
                if *caller != escrow && !admin_ok {
                    panic!("escrow or admin only");
                }
            }
            (&VerificationPending, &OraclePending) => {
                if !t.verification_ok {
                    panic!("verification not ok");
                }
                if !initiator_ok && !admin_ok {
                    panic!("initiator or admin");
                }
            }
            (&OraclePending, &ReleasePending) => {
                if !t.oracle_ok {
                    panic!("oracle not ok");
                }
                if !initiator_ok && !admin_ok {
                    panic!("initiator or admin");
                }
            }
            (_, &Failed) => {
                if !initiator_ok && !admin_ok {
                    panic!("initiator or admin");
                }
            }
            _ => {
                if !initiator_ok && !admin_ok {
                    panic!("initiator or admin only");
                }
            }
        }
    }

    fn apply_transition(
        env: &Env,
        t: &mut BridgeTransfer,
        actor: &Address,
        from: &TransferState,
        to: &TransferState,
        note: String,
    ) {
        let now = env.ledger().timestamp();
        t.state = to.clone();
        t.updated_at = now;
        if !Self::is_terminal(to) {
            t.state_deadline = now + Self::timeout_secs_for(env, to);
        }
        env.storage().persistent().set(&DataKey::Transfer(t.id), t);

        let mut hist: Vec<StateTransitionLog> = env
            .storage()
            .persistent()
            .get(&DataKey::History(t.id))
            .unwrap_or_else(|| Vec::new(env));
        if hist.len() >= MAX_HISTORY_PER_TRANSFER {
            panic!("history overflow");
        }
        hist.push_back(StateTransitionLog {
            from_state: from.clone(),
            to_state: to.clone(),
            timestamp: now,
            actor: actor.clone(),
            note,
        });
        env.storage()
            .persistent()
            .set(&DataKey::History(t.id), &hist);

        env.events()
            .publish((EVT_ADV, t.id), (from.clone(), to.clone()));
    }

    fn is_admin(env: &Env, caller: &Address) -> bool {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        *caller == admin
    }

    fn assert_admin(env: &Env, caller: &Address) {
        caller.require_auth();
        if !Self::is_admin(env, caller) {
            panic!("admin only");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    fn setup() -> (
        Env,
        TransferStateMachineClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TransferStateMachine);
        let client = TransferStateMachineClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let escrow = Address::generate(&env);
        let verifier = Address::generate(&env);
        let oracle = Address::generate(&env);
        client.initialize(&admin);
        client.set_integration_contracts(&admin, &escrow, &verifier, &oracle);
        (env, client, admin, escrow, verifier, oracle)
    }

    #[test]
    fn test_initiate_and_verify_happy_path_edges() {
        let (env, client, _admin, escrow, verifier, oracle) = setup();
        env.ledger().set_timestamp(1_000);
        let user = Address::generate(&env);
        let asset = String::from_str(&env, "USDC");
        let dest = String::from_str(&env, "eth:0xabc");

        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &asset,
            &1000i128,
            &dest,
        );

        let t = client.get_transfer(&id).unwrap();
        assert_eq!(t.state, TransferState::Initiated);
        assert!(client.verify_transition(
            &id,
            &TransferState::Initiated,
            &TransferState::AwaitingSource
        ));

        client.advance_state(&user, &id, &TransferState::AwaitingSource);
        client.advance_state(&user, &id, &TransferState::EscrowPending);
        client.advance_state(&escrow, &id, &TransferState::EscrowLocked);
        client.advance_state(&user, &id, &TransferState::VerificationPending);

        client.submit_verification_result(&verifier, &id, &true);
        client.try_auto_progress(&id);
        let t2 = client.get_transfer(&id).unwrap();
        assert_eq!(t2.state, TransferState::OraclePending);

        client.submit_oracle_result(&oracle, &id, &true);
        client.try_auto_progress(&id);
        let t3 = client.get_transfer(&id).unwrap();
        assert_eq!(t3.state, TransferState::ReleasePending);

        client.advance_state(&user, &id, &TransferState::Completed);
        let fin = client.get_transfer(&id).unwrap();
        assert_eq!(fin.state, TransferState::Completed);
        assert!(client.get_transition_history(&id).len() >= 6);
    }

    #[test]
    fn test_fast_track_initiated_to_escrow_pending() {
        let (env, client, _admin, _escrow, _v, _o) = setup();
        env.ledger().set_timestamp(10_000);
        let user = Address::generate(&env);

        let id = client.initiate_transfer(
            &user,
            &BridgeType::LockMint,
            &TransferMode::FastTrack,
            &String::from_str(&env, "XLM"),
            &500i128,
            &String::from_str(&env, "memo"),
        );
        assert!(client.verify_transition(
            &id,
            &TransferState::Initiated,
            &TransferState::EscrowPending
        ));
        client.advance_state(&user, &id, &TransferState::EscrowPending);
    }

    #[test]
    fn test_standard_cannot_skip_awaiting_source() {
        let (env, client, _a, _e, _v, _o) = setup();
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Custom,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        assert!(!client.verify_transition(
            &id,
            &TransferState::Initiated,
            &TransferState::EscrowPending
        ));
    }

    #[test]
    fn test_handle_timeout() {
        let (env, client, _a, _e, _v, _o) = setup();
        env.ledger().set_timestamp(100);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &10i128,
            &String::from_str(&env, "d"),
        );
        // Default Initiated timeout 3600; jump past deadline
        env.ledger().set_timestamp(10_000);
        client.handle_timeout(&id);
        assert_eq!(
            client.get_transfer(&id).unwrap().state,
            TransferState::TimedOut
        );
    }

    #[test]
    #[should_panic(expected = "timeout not reached")]
    fn test_handle_timeout_premature_panics() {
        let (env, client, _a, _e, _v, _o) = setup();
        env.ledger().set_timestamp(100);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &10i128,
            &String::from_str(&env, "d"),
        );
        client.handle_timeout(&id);
    }

    #[test]
    fn test_rollback_by_initiator() {
        let (env, client, _a, escrow, _v, _o) = setup();
        env.ledger().set_timestamp(1_000);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::BurnRelease,
            &TransferMode::Standard,
            &String::from_str(&env, "EURC"),
            &99i128,
            &String::from_str(&env, "d"),
        );
        client.advance_state(&user, &id, &TransferState::AwaitingSource);
        client.advance_state(&user, &id, &TransferState::EscrowPending);
        client.advance_state(&escrow, &id, &TransferState::EscrowLocked);
        client.rollback_transfer(&user, &id, &String::from_str(&env, "user requested"));
        assert_eq!(
            client.get_transfer(&id).unwrap().state,
            TransferState::RolledBack
        );
    }

    #[test]
    fn test_admin_override_from_timed_out() {
        let (env, client, admin, _e, _v, _o) = setup();
        env.ledger().set_timestamp(100);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::NativeWrapped,
            &TransferMode::Insured,
            &String::from_str(&env, "PYUSD"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        env.ledger().set_timestamp(10_000);
        client.handle_timeout(&id);
        client.admin_override_state(
            &admin,
            &id,
            &TransferState::EscrowLocked,
            &String::from_str(&env, "manual recovery"),
        );
        assert_eq!(
            client.get_transfer(&id).unwrap().state,
            TransferState::EscrowLocked
        );
    }

    #[test]
    #[should_panic(expected = "only verifier contract")]
    fn test_submit_verification_wrong_caller() {
        let (env, client, _a, escrow, _v, _o) = setup();
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        client.advance_state(&user, &id, &TransferState::AwaitingSource);
        client.advance_state(&user, &id, &TransferState::EscrowPending);
        client.advance_state(&escrow, &id, &TransferState::EscrowLocked);
        client.advance_state(&user, &id, &TransferState::VerificationPending);
        client.submit_verification_result(&user, &id, &true);
    }

    #[test]
    fn test_advance_to_failed() {
        let (env, client, _a, _e, _v, _o) = setup();
        env.ledger().set_timestamp(1);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        client.advance_state(&user, &id, &TransferState::Failed);
        assert_eq!(
            client.get_transfer(&id).unwrap().state,
            TransferState::Failed
        );
    }

    #[test]
    fn test_set_state_timeout_affects_future_deadline() {
        let (env, client, admin, _e, _v, _o) = setup();
        client.set_state_timeout(&admin, &TransferState::Initiated, &50u64);
        env.ledger().set_timestamp(1000);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        let t = client.get_transfer(&id).unwrap();
        assert_eq!(t.state_deadline, 1000 + 50);
    }

    #[test]
    fn test_get_timeout_for_state() {
        let (_env, client, admin, _e, _v, _o) = setup();
        client.set_state_timeout(&admin, &TransferState::OraclePending, &123u64);
        assert_eq!(
            client.get_timeout_for_state(&TransferState::OraclePending),
            123
        );
    }

    #[test]
    #[should_panic(expected = "invalid transition")]
    fn test_advance_invalid_transition_panics() {
        let (env, client, _a, _e, _v, _o) = setup();
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        // Skip to Completed
        client.advance_state(&user, &id, &TransferState::Completed);
    }

    #[test]
    fn test_verify_transition_false_when_wrong_current() {
        let (env, client, _a, _e, _v, _o) = setup();
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        assert!(!client.verify_transition(
            &id,
            &TransferState::EscrowLocked,
            &TransferState::Completed
        ));
    }

    #[test]
    fn test_history_log_order() {
        let (env, client, _a, _e, _v, _o) = setup();
        env.ledger().set_timestamp(500);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        client.advance_state(&user, &id, &TransferState::AwaitingSource);
        let h = client.get_transition_history(&id);
        assert_eq!(h.len(), 1);
        let row = h.get(0).unwrap();
        assert_eq!(row.from_state, TransferState::Initiated);
        assert_eq!(row.to_state, TransferState::AwaitingSource);
    }

    #[test]
    #[should_panic(expected = "wrong state for verification")]
    fn test_verifier_wrong_state_panics() {
        let (env, client, _a, _e, verifier, _o) = setup();
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        client.submit_verification_result(&verifier, &id, &true);
    }

    #[test]
    fn test_oracle_fail_stops_auto_complete() {
        let (env, client, _a, escrow, verifier, oracle) = setup();
        env.ledger().set_timestamp(1_000);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        client.advance_state(&user, &id, &TransferState::AwaitingSource);
        client.advance_state(&user, &id, &TransferState::EscrowPending);
        client.advance_state(&escrow, &id, &TransferState::EscrowLocked);
        client.advance_state(&user, &id, &TransferState::VerificationPending);
        client.submit_verification_result(&verifier, &id, &true);
        client.try_auto_progress(&id);
        client.submit_oracle_result(&oracle, &id, &false);
        client.try_auto_progress(&id);
        assert_eq!(
            client.get_transfer(&id).unwrap().state,
            TransferState::OraclePending
        );
    }

    #[test]
    fn test_get_transfer_unknown_returns_none() {
        let (_env, client, _a, _e, _v, _o) = setup();
        assert!(client.get_transfer(&999_u64).is_none());
    }

    #[test]
    #[should_panic(expected = "unauthorized rollback")]
    fn test_rollback_stranger_panics() {
        let (env, client, _a, escrow, _v, _o) = setup();
        env.ledger().set_timestamp(1_000);
        let user = Address::generate(&env);
        let stranger = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        client.advance_state(&user, &id, &TransferState::AwaitingSource);
        client.advance_state(&user, &id, &TransferState::EscrowPending);
        client.advance_state(&escrow, &id, &TransferState::EscrowLocked);
        client.rollback_transfer(&stranger, &id, &String::from_str(&env, "no"));
    }

    #[test]
    #[should_panic(expected = "only oracle contract")]
    fn test_submit_oracle_wrong_caller() {
        let (env, client, _a, escrow, verifier, _o) = setup();
        env.ledger().set_timestamp(1_000);
        let user = Address::generate(&env);
        let id = client.initiate_transfer(
            &user,
            &BridgeType::Cctp,
            &TransferMode::Standard,
            &String::from_str(&env, "USDC"),
            &1i128,
            &String::from_str(&env, "d"),
        );
        client.advance_state(&user, &id, &TransferState::AwaitingSource);
        client.advance_state(&user, &id, &TransferState::EscrowPending);
        client.advance_state(&escrow, &id, &TransferState::EscrowLocked);
        client.advance_state(&user, &id, &TransferState::VerificationPending);
        client.submit_verification_result(&verifier, &id, &true);
        client.try_auto_progress(&id);
        client.submit_oracle_result(&user, &id, &true);
    }

    #[test]
    fn test_bridge_type_variants_used() {
        let (env, client, _a, _e, _v, _o) = setup();
        let user = Address::generate(&env);
        for (bt, m) in [
            (BridgeType::LockMint, TransferMode::Standard),
            (BridgeType::BurnRelease, TransferMode::Insured),
            (BridgeType::NativeWrapped, TransferMode::FastTrack),
            (BridgeType::Cctp, TransferMode::Standard),
            (BridgeType::Custom, TransferMode::FastTrack),
        ]
        .iter()
        {
            client.initiate_transfer(
                &user,
                bt,
                m,
                &String::from_str(&env, "A"),
                &1i128,
                &String::from_str(&env, "d"),
            );
        }
    }
}
