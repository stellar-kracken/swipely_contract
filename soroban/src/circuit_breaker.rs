use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, String, Vec};

// ── Enums ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PauseLevel {
    None,
    Warning, // Reduced limits, notifications
    Partial, // Some operations paused
    Full,    // All operations paused
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GuardianRole {
    StandardGuardian,  // Can approve pauses
    EmergencyGuardian, // Can trigger emergency pauses
    AdminGuardian,     // Can manage guardians
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AlertType {
    HealthScore,
    PriceDeviation,
    SupplyMismatch,
    BridgeDowntime,
    VolumeSpike,
    ReserveRatio,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PauseScope {
    Global,
    Bridge(String),
    Asset(String),
}

// ── Structs ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct PauseState {
    pub scope: PauseScope,
    pub level: PauseLevel,
    pub triggered_by: Address,
    pub trigger_reason: String,
    pub timestamp: u64,
    pub recovery_deadline: u64,
    pub guardian_approvals: u32,
    pub guardian_threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct TriggerConfig {
    pub alert_type: AlertType,
    pub threshold: i128,
    pub pause_level: PauseLevel,
    pub cooldown_period: u64,
    pub last_trigger: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct GuardianInfo {
    pub address: Address,
    pub role: GuardianRole,
    pub added_at: u64,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RecoveryRequest {
    pub pause_id: u32,
    pub requested_by: Address,
    pub timestamp: u64,
    pub approvals: u32,
    pub threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    pub admin: Address,
    pub guardian_threshold: u32,
    pub recovery_delay_warning: u64,
    pub recovery_delay_partial: u64,
    pub recovery_delay_full: u64,
    pub max_whitelist_size: u32,
}

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Config,
    PauseCount,
    PauseState(u32),
    TriggerConfigs,
    Guardians,
    WhitelistAddresses,
    WhitelistAssets,
    WhitelistBridges,
    RecoveryRequests,
    GuardianApprovals(u32),
    RecoveryApprovals(u32),
}

// ── Events ────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
const EVENT_PAUSE_TRIGGERED: &str = "cb_pause_triggered";
#[allow(dead_code)]
const EVENT_PAUSE_LIFTED: &str = "cb_pause_lifted";
#[allow(dead_code)]
const EVENT_GUARDIAN_ADDED: &str = "cb_guardian_added";
#[allow(dead_code)]
const EVENT_GUARDIAN_REMOVED: &str = "cb_guardian_removed";
#[allow(dead_code)]
const EVENT_GUARDIAN_APPROVED: &str = "cb_guardian_approved";
#[allow(dead_code)]
const EVENT_RECOVERY_REQUESTED: &str = "cb_recovery_requested";
#[allow(dead_code)]
const EVENT_RECOVERY_EXECUTED: &str = "cb_recovery_executed";
#[allow(dead_code)]
const EVENT_TRIGGER_CONFIG_UPDATED: &str = "cb_trigger_updated";
#[allow(dead_code)]
const EVENT_WHITELIST_UPDATED: &str = "cb_whitelist_updated";

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct CircuitBreakerContract;

#[contractimpl]
impl CircuitBreakerContract {
    // ── Initialization ────────────────────────────────────────────────────────

    pub fn initialize(
        env: Env,
        admin: Address,
        guardian_threshold: u32,
        recovery_delay_warning: u64,
        recovery_delay_partial: u64,
        recovery_delay_full: u64,
        max_whitelist_size: u32,
    ) {
        admin.require_auth();
        assert!(
            !env.storage().instance().has(&DataKey::Config),
            "already initialized"
        );

        let config = CircuitBreakerConfig {
            admin,
            guardian_threshold,
            recovery_delay_warning,
            recovery_delay_partial,
            recovery_delay_full,
            max_whitelist_size,
        };

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::PauseCount, &0u32);

        // Initialize empty collections
        let trigger_configs: Map<AlertType, TriggerConfig> = Map::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::TriggerConfigs, &trigger_configs);

        let guardians: Vec<GuardianInfo> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::Guardians, &guardians);

        let whitelist_addresses: Vec<Address> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::WhitelistAddresses, &whitelist_addresses);

        let whitelist_assets: Vec<String> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::WhitelistAssets, &whitelist_assets);

        let whitelist_bridges: Vec<String> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::WhitelistBridges, &whitelist_bridges);

        let recovery_requests: Vec<RecoveryRequest> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::RecoveryRequests, &recovery_requests);
    }

    // ── Guardian Management ───────────────────────────────────────────────────

    pub fn add_guardian(env: Env, caller: Address, guardian: Address, role: GuardianRole) {
        Self::only_admin(&env, &caller);

        let mut guardians: Vec<GuardianInfo> = env
            .storage()
            .instance()
            .get(&DataKey::Guardians)
            .unwrap_or(Vec::new(&env));

        // Check if already exists
        for g in guardians.iter() {
            if g.address == guardian {
                panic!("guardian already exists");
            }
        }

        let info = GuardianInfo {
            address: guardian.clone(),
            role: role.clone(),
            added_at: env.ledger().timestamp(),
            active: true,
        };

        guardians.push_back(info);
        env.storage()
            .instance()
            .set(&DataKey::Guardians, &guardians);

        env.events()
            .publish(("cb_guardian_added",), (guardian, role));
    }

    pub fn remove_guardian(env: Env, caller: Address, guardian: Address) {
        Self::only_admin(&env, &caller);

        let guardians: Vec<GuardianInfo> = env
            .storage()
            .instance()
            .get(&DataKey::Guardians)
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        let mut new_guardians: Vec<GuardianInfo> = Vec::new(&env);

        for g in guardians.iter() {
            if g.address != guardian {
                new_guardians.push_back(g);
            } else {
                found = true;
            }
        }

        assert!(found, "guardian not found");

        env.storage()
            .instance()
            .set(&DataKey::Guardians, &new_guardians);

        env.events().publish(("cb_guardian_removed",), guardian);
    }

    pub fn get_guardians(env: Env) -> Vec<GuardianInfo> {
        env.storage()
            .instance()
            .get(&DataKey::Guardians)
            .unwrap_or(Vec::new(&env))
    }

    // ── Pause Operations ──────────────────────────────────────────────────────

    pub fn pause_global(env: Env, caller: Address, reason: String) {
        Self::check_guardian_permission(&env, &caller, GuardianRole::EmergencyGuardian);

        let pause_id = Self::get_next_pause_id(&env);
        let config = Self::get_config(&env);
        let recovery_delay = config.recovery_delay_full;

        let pause_state = PauseState {
            scope: PauseScope::Global,
            level: PauseLevel::Full,
            triggered_by: caller,
            trigger_reason: reason,
            timestamp: env.ledger().timestamp(),
            recovery_deadline: env.ledger().timestamp() + recovery_delay,
            guardian_approvals: 1, // Auto-approve for emergency guardian
            guardian_threshold: config.guardian_threshold,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PauseState(pause_id), &pause_state);

        env.events().publish(
            ("cb_pause_triggered",),
            (pause_id, PauseScope::Global, PauseLevel::Full),
        );
    }

    pub fn pause_bridge(env: Env, caller: Address, bridge_id: String, reason: String) {
        Self::check_guardian_permission(&env, &caller, GuardianRole::StandardGuardian);

        let pause_id = Self::get_next_pause_id(&env);
        let config = Self::get_config(&env);
        let recovery_delay = config.recovery_delay_partial;

        let pause_state = PauseState {
            scope: PauseScope::Bridge(bridge_id.clone()),
            level: PauseLevel::Partial,
            triggered_by: caller,
            trigger_reason: reason,
            timestamp: env.ledger().timestamp(),
            recovery_deadline: env.ledger().timestamp() + recovery_delay,
            guardian_approvals: 1,
            guardian_threshold: config.guardian_threshold,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PauseState(pause_id), &pause_state);

        env.events().publish(
            ("cb_pause_triggered",),
            (pause_id, PauseScope::Bridge(bridge_id), PauseLevel::Partial),
        );
    }

    pub fn pause_asset(env: Env, caller: Address, asset_code: String, reason: String) {
        Self::check_guardian_permission(&env, &caller, GuardianRole::StandardGuardian);

        let pause_id = Self::get_next_pause_id(&env);
        let config = Self::get_config(&env);
        let recovery_delay = config.recovery_delay_warning;

        let pause_state = PauseState {
            scope: PauseScope::Asset(asset_code.clone()),
            level: PauseLevel::Warning,
            triggered_by: caller,
            trigger_reason: reason,
            timestamp: env.ledger().timestamp(),
            recovery_deadline: env.ledger().timestamp() + recovery_delay,
            guardian_approvals: 1,
            guardian_threshold: config.guardian_threshold,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PauseState(pause_id), &pause_state);

        env.events().publish(
            ("cb_pause_triggered",),
            (pause_id, PauseScope::Asset(asset_code), PauseLevel::Warning),
        );
    }

    // ── Recovery Operations ───────────────────────────────────────────────────

    pub fn request_recovery(env: Env, caller: Address, pause_id: u32) {
        Self::check_guardian_permission(&env, &caller, GuardianRole::StandardGuardian);

        let pause_state = Self::get_pause_state(env.clone(), pause_id);
        assert!(pause_state.level != PauseLevel::None, "pause not active");

        let mut recovery_requests: Vec<RecoveryRequest> = env
            .storage()
            .instance()
            .get(&DataKey::RecoveryRequests)
            .unwrap_or(Vec::new(&env));

        // Check if request already exists
        for req in recovery_requests.iter() {
            if req.pause_id == pause_id {
                panic!("recovery already requested");
            }
        }

        let config = Self::get_config(&env);
        let request = RecoveryRequest {
            pause_id,
            requested_by: caller.clone(),
            timestamp: env.ledger().timestamp(),
            approvals: 1,
            threshold: config.guardian_threshold,
        };

        recovery_requests.push_back(request);
        env.storage()
            .instance()
            .set(&DataKey::RecoveryRequests, &recovery_requests);

        env.events()
            .publish(("cb_recovery_requested",), (pause_id, caller));
    }

    pub fn approve_recovery(env: Env, caller: Address, pause_id: u32) {
        Self::check_guardian_permission(&env, &caller, GuardianRole::StandardGuardian);

        let mut recovery_requests: Vec<RecoveryRequest> = env
            .storage()
            .instance()
            .get(&DataKey::RecoveryRequests)
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        let len = recovery_requests.len();
        for i in 0..len {
            let mut req = recovery_requests.get(i).unwrap();
            if req.pause_id == pause_id {
                req.approvals += 1;
                recovery_requests.set(i, req);
                found = true;
                break;
            }
        }

        assert!(found, "recovery request not found");

        env.storage()
            .instance()
            .set(&DataKey::RecoveryRequests, &recovery_requests);

        env.events()
            .publish(("cb_guardian_approved",), (pause_id, caller, "recovery"));
    }

    pub fn execute_recovery(env: Env, caller: Address, pause_id: u32) {
        Self::check_guardian_permission(&env, &caller, GuardianRole::StandardGuardian);

        let mut recovery_requests: Vec<RecoveryRequest> = env
            .storage()
            .instance()
            .get(&DataKey::RecoveryRequests)
            .unwrap_or(Vec::new(&env));

        let mut request_index = None;
        let mut can_execute = false;

        for (i, req) in recovery_requests.iter().enumerate() {
            if req.pause_id == pause_id {
                if req.approvals >= req.threshold {
                    can_execute = true;
                    request_index = Some(i);
                }
                break;
            }
        }

        assert!(can_execute, "insufficient approvals");

        // Remove the request
        if let Some(index) = request_index {
            recovery_requests.remove(index as u32);
        }

        env.storage()
            .instance()
            .set(&DataKey::RecoveryRequests, &recovery_requests);

        // Clear the pause state
        env.storage()
            .persistent()
            .remove(&DataKey::PauseState(pause_id));

        env.events().publish(("cb_recovery_executed",), pause_id);
    }

    // ── Trigger Configuration ─────────────────────────────────────────────────

    pub fn set_trigger_config(
        env: Env,
        caller: Address,
        alert_type: AlertType,
        threshold: i128,
        pause_level: PauseLevel,
        cooldown_period: u64,
    ) {
        Self::only_admin(&env, &caller);

        let mut trigger_configs: Map<AlertType, TriggerConfig> = env
            .storage()
            .instance()
            .get(&DataKey::TriggerConfigs)
            .unwrap_or(Map::new(&env));

        let config = TriggerConfig {
            alert_type: alert_type.clone(),
            threshold,
            pause_level: pause_level.clone(),
            cooldown_period,
            last_trigger: 0,
        };

        trigger_configs.set(alert_type.clone(), config);
        env.storage()
            .instance()
            .set(&DataKey::TriggerConfigs, &trigger_configs);

        env.events().publish(
            ("cb_trigger_updated",),
            (alert_type, threshold, pause_level),
        );
    }

    pub fn add_to_address_whitelist(env: Env, caller: Address, address: Address) {
        Self::only_admin(&env, &caller);

        let mut whitelist: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::WhitelistAddresses)
            .unwrap_or(Vec::new(&env));

        let config = Self::get_config(&env);
        assert!(
            whitelist.len() < config.max_whitelist_size,
            "whitelist full"
        );

        // Check if already exists
        for addr in whitelist.iter() {
            if addr == address {
                panic!("address already whitelisted");
            }
        }

        whitelist.push_back(address.clone());
        env.storage()
            .instance()
            .set(&DataKey::WhitelistAddresses, &whitelist);

        env.events()
            .publish(("cb_whitelist_updated",), ("address", address, true));
    }

    pub fn add_asset_to_whitelist(env: Env, caller: Address, asset_code: String) {
        Self::only_admin(&env, &caller);

        let mut whitelist: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::WhitelistAssets)
            .unwrap_or(Vec::new(&env));

        let config = Self::get_config(&env);
        assert!(
            whitelist.len() < config.max_whitelist_size,
            "whitelist full"
        );

        // Check if already exists
        for asset in whitelist.iter() {
            if asset == asset_code {
                panic!("asset already whitelisted");
            }
        }

        whitelist.push_back(asset_code.clone());
        env.storage()
            .instance()
            .set(&DataKey::WhitelistAssets, &whitelist);

        env.events()
            .publish(("cb_whitelist_updated",), ("asset", asset_code, true));
    }

    // ── Query Functions ───────────────────────────────────────────────────────

    pub fn get_pause_state(env: Env, pause_id: u32) -> PauseState {
        env.storage()
            .persistent()
            .get(&DataKey::PauseState(pause_id))
            .unwrap_or(PauseState {
                scope: PauseScope::Global,
                level: PauseLevel::None,
                triggered_by: env.current_contract_address(),
                trigger_reason: String::from_str(&env, ""),
                timestamp: 0,
                recovery_deadline: 0,
                guardian_approvals: 0,
                guardian_threshold: 0,
            })
    }

    pub fn is_paused(env: Env, scope: PauseScope) -> bool {
        let pause_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PauseCount)
            .unwrap_or(0);

        for i in 1..=pause_count {
            let pause_state = Self::get_pause_state(env.clone(), i);
            if pause_state.level != PauseLevel::None {
                match (&pause_state.scope, &scope) {
                    (PauseScope::Global, _) => return true,
                    (PauseScope::Bridge(id1), PauseScope::Bridge(id2)) if id1 == id2 => {
                        return true
                    }
                    (PauseScope::Asset(code1), PauseScope::Asset(code2)) if code1 == code2 => {
                        return true
                    }
                    _ => continue,
                }
            }
        }
        false
    }

    pub fn is_whitelisted_address(env: Env, address: Address) -> bool {
        let whitelist: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::WhitelistAddresses)
            .unwrap_or(Vec::new(&env));

        for addr in whitelist.iter() {
            if addr == address {
                return true;
            }
        }
        false
    }

    pub fn is_whitelisted_asset(env: Env, asset_code: String) -> bool {
        let whitelist: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::WhitelistAssets)
            .unwrap_or(Vec::new(&env));

        for asset in whitelist.iter() {
            if asset == asset_code {
                return true;
            }
        }
        false
    }

    // ── Helper Functions ──────────────────────────────────────────────────────

    fn get_config(env: &Env) -> CircuitBreakerConfig {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("contract not initialized")
    }

    fn get_next_pause_id(env: &Env) -> u32 {
        let mut count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PauseCount)
            .unwrap_or(0);
        count += 1;
        env.storage().instance().set(&DataKey::PauseCount, &count);
        count
    }

    fn only_admin(env: &Env, caller: &Address) {
        let config = Self::get_config(env);
        assert!(caller == &config.admin, "not admin");
        caller.require_auth();
    }

    fn check_guardian_permission(env: &Env, caller: &Address, required_role: GuardianRole) {
        let guardians: Vec<GuardianInfo> = env
            .storage()
            .instance()
            .get(&DataKey::Guardians)
            .unwrap_or(Vec::new(env));

        for guardian in guardians.iter() {
            if guardian.address == *caller && guardian.active {
                match (&guardian.role, &required_role) {
                    (GuardianRole::AdminGuardian, _) => return,
                    (GuardianRole::EmergencyGuardian, GuardianRole::EmergencyGuardian) => return,
                    (GuardianRole::EmergencyGuardian, GuardianRole::StandardGuardian) => return,
                    (GuardianRole::StandardGuardian, GuardianRole::StandardGuardian) => return,
                    _ => continue,
                }
            }
        }
        panic!("insufficient guardian permissions");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn create_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn create_test_addresses(env: &Env) -> (Address, Address, Address, Address) {
        let admin = Address::generate(env);
        let guardian1 = Address::generate(env);
        let guardian2 = Address::generate(env);
        let user = Address::generate(env);
        (admin, guardian1, guardian2, user)
    }

    fn setup() -> (
        Env,
        Address,
        CircuitBreakerContractClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let env = create_test_env();
        let (admin, guardian1, guardian2, user) = create_test_addresses(&env);
        let contract_id = env.register(CircuitBreakerContract, ());
        let client = CircuitBreakerContractClient::new(&env, &contract_id);
        (env, contract_id, client, admin, guardian1, guardian2, user)
    }

    #[test]
    fn test_initialization() {
        let (env, contract_id, client, admin, _, _, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);

        let has_config = env.as_contract(&contract_id, || {
            env.storage().instance().has(&DataKey::Config)
        });
        assert!(has_config);
    }

    #[test]
    #[should_panic]
    fn test_double_initialization() {
        let (_env, _contract_id, client, admin, _, _, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
    }

    #[test]
    fn test_add_guardian() {
        let (_env, _contract_id, client, admin, guardian1, _, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_guardian(&admin, &guardian1, &GuardianRole::StandardGuardian);

        let guardians = client.get_guardians();
        assert_eq!(guardians.len(), 1);
        assert_eq!(guardians.get(0).unwrap().address, guardian1);
        assert_eq!(
            guardians.get(0).unwrap().role,
            GuardianRole::StandardGuardian
        );
    }

    #[test]
    #[should_panic]
    fn test_add_guardian_non_admin() {
        let (_env, _contract_id, client, admin, guardian1, _, user) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_guardian(&user, &guardian1, &GuardianRole::StandardGuardian);
    }

    #[test]
    fn test_pause_global() {
        let (env, _contract_id, client, admin, guardian1, _, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_guardian(&admin, &guardian1, &GuardianRole::EmergencyGuardian);

        let reason = String::from_str(&env, "Test global pause");
        client.pause_global(&guardian1, &reason);

        assert!(client.is_paused(&PauseScope::Global));
    }

    #[test]
    fn test_pause_bridge() {
        let (env, _contract_id, client, admin, guardian1, _, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_guardian(&admin, &guardian1, &GuardianRole::StandardGuardian);

        let bridge_id = String::from_str(&env, "test-bridge");
        let reason = String::from_str(&env, "Test bridge pause");
        client.pause_bridge(&guardian1, &bridge_id, &reason);

        assert!(client.is_paused(&PauseScope::Bridge(bridge_id)));
    }

    #[test]
    fn test_pause_asset() {
        let (env, _contract_id, client, admin, guardian1, _, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_guardian(&admin, &guardian1, &GuardianRole::StandardGuardian);

        let asset_code = String::from_str(&env, "USDC");
        let reason = String::from_str(&env, "Test asset pause");
        client.pause_asset(&guardian1, &asset_code, &reason);

        assert!(client.is_paused(&PauseScope::Asset(asset_code)));
    }

    #[test]
    #[should_panic]
    fn test_pause_non_guardian() {
        let (env, _contract_id, client, admin, _guardian1, _guardian2, user) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);

        let reason = String::from_str(&env, "Test pause");
        client.pause_global(&user, &reason);
    }

    #[test]
    fn test_recovery_flow() {
        let (env, _contract_id, client, admin, guardian1, guardian2, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_guardian(&admin, &guardian1, &GuardianRole::EmergencyGuardian);
        client.add_guardian(&admin, &guardian2, &GuardianRole::StandardGuardian);

        let reason = String::from_str(&env, "Test pause");
        client.pause_global(&guardian1, &reason);
        client.request_recovery(&guardian2, &1);
        client.approve_recovery(&guardian1, &1);
        client.execute_recovery(&guardian2, &1);

        assert!(!client.is_paused(&PauseScope::Global));
    }

    #[test]
    fn test_whitelist_address() {
        let (_env, _contract_id, client, admin, _, _, user) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_to_address_whitelist(&admin, &user);

        assert!(client.is_whitelisted_address(&user));
    }

    #[test]
    fn test_whitelist_asset() {
        let (env, _contract_id, client, admin, _, _, _) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);

        let asset_code = String::from_str(&env, "USDC");
        client.add_asset_to_whitelist(&admin, &asset_code);

        assert!(client.is_whitelisted_asset(&asset_code));
    }

    #[test]
    #[should_panic]
    fn test_whitelist_non_admin() {
        let (_env, _contract_id, client, admin, guardian1, _, user) = setup();

        client.initialize(&admin, &2, &3600, &7200, &14400, &100);
        client.add_to_address_whitelist(&user, &guardian1);
    }
}
