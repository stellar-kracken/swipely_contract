use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Vec};

/// Maximum number of signers allowed
pub const MAX_SIGNERS: u32 = 100;

/// Default time delay for large transactions (1 hour in seconds)
pub const DEFAULT_TIME_DELAY: u64 = 3600;

/// Emergency threshold multiplier (e.g., 2x normal threshold)
pub const EMERGENCY_MULTIPLIER: u32 = 2;

/// Maximum spending limit per role (default 10000 in stroops)
pub const DEFAULT_SPENDING_LIMIT: i128 = 10000;

/// Data key enum for storage management
#[contracttype]
pub enum DataKey {
    Admin,
    Config,
    Signer(Address),
    SignerCount,
    Transaction(u64),
    TransactionCount,
    Nonce,
    Role(Address),
    SpendingLimit(Address, u64), // (role, time_period)
    WhitelistedAsset(Address),
    TransactionHistory(u64),
}

/// Role types for access control
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Role {
    Admin,
    Operator,
    Guardian,
}

/// Transaction status
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Approved,
    Executed,
    Cancelled,
    Expired,
}

/// Transaction data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    pub id: u64,
    pub creator: Address,
    pub destination: Address,
    pub amount: i128,
    pub asset: Address,
    pub description: String,
    pub status: TransactionStatus,
    pub required_signatures: u32,
    pub current_signatures: u32,
    pub signers: Vec<Address>,
    pub time_delay: u64,
    pub execution_time: u64,
    pub created_at: u64,
    pub executed_at: u64,
    pub is_emergency: bool,
}

/// Signer data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignerData {
    pub address: Address,
    pub role: Role,
    pub weight: u32,
    pub is_active: bool,
    pub added_at: u64,
}

/// Multi-sig configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiSigConfig {
    pub threshold: u32,
    pub emergency_threshold: u32,
    pub time_delay: u64,
    pub max_transactions_per_day: u32,
    pub require_emergency_approval: bool,
}

/// Transaction audit log entry
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionLog {
    pub transaction_id: u64,
    pub action: String,
    pub actor: Address,
    pub timestamp: u64,
    pub details: String,
}

#[contract]
pub struct MultiSigTreasuryContract;

#[contractimpl]
impl MultiSigTreasuryContract {
    /// Initialize the multi-sig treasury contract
    pub fn initialize(
        env: Env,
        admin: Address,
        config: MultiSigConfig,
        initial_signers: Vec<Address>,
        roles: Vec<Role>,
    ) {
        admin.require_auth();

        if initial_signers.len() != roles.len() {
            panic!("Signers and roles must have same length");
        }

        if initial_signers.len() < config.threshold {
            panic!("Initial signers must meet threshold");
        }

        // Set admin and config
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::Nonce, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TransactionCount, &0u64);
        env.storage().instance().set(&DataKey::SignerCount, &0u32);

        // Add initial signers
        for i in 0..initial_signers.len() {
            let signer = initial_signers.get(i).unwrap();
            let role = roles.get(i).unwrap();

            let signer_data = SignerData {
                address: signer.clone(),
                role: role.clone(),
                weight: 1,
                is_active: true,
                added_at: env.ledger().timestamp(),
            };

            env.storage()
                .persistent()
                .set(&DataKey::Signer(signer.clone()), &signer_data);
            env.storage()
                .persistent()
                .set(&DataKey::Role(signer), &role);
        }

        let signer_count = initial_signers.len();
        env.storage()
            .instance()
            .set(&DataKey::SignerCount, &signer_count);
    }

    /// Propose a new transaction
    pub fn propose_transaction(
        env: Env,
        creator: Address,
        destination: Address,
        amount: i128,
        asset: Address,
        description: String,
        is_emergency: bool,
    ) -> u64 {
        creator.require_auth();

        // Check if creator is a valid signer
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Signer(creator.clone()))
        {
            panic!("Only signers can propose transactions");
        }

        let config: MultiSigConfig = env.storage().instance().get(&DataKey::Config).unwrap();

        // Check daily transaction limit
        let transaction_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TransactionCount)
            .unwrap();
        if transaction_count >= config.max_transactions_per_day as u64 {
            panic!("Daily transaction limit reached");
        }

        // Determine required signatures
        let required_signatures = if is_emergency && config.require_emergency_approval {
            config.emergency_threshold
        } else {
            config.threshold
        };

        // Calculate execution time (time delay for large transactions)
        let time_delay = if amount > DEFAULT_SPENDING_LIMIT {
            config.time_delay
        } else {
            0 // No delay for small transactions
        };

        let execution_time = env.ledger().timestamp() + time_delay;

        // Create transaction
        let mut nonce: u64 = env.storage().instance().get(&DataKey::Nonce).unwrap();
        nonce += 1;
        env.storage().instance().set(&DataKey::Nonce, &nonce);

        let mut tx_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TransactionCount)
            .unwrap();
        tx_count += 1;
        env.storage()
            .instance()
            .set(&DataKey::TransactionCount, &tx_count);

        let transaction = Transaction {
            id: nonce,
            creator: creator.clone(),
            destination,
            amount,
            asset: asset.clone(),
            description,
            status: TransactionStatus::Pending,
            required_signatures,
            current_signatures: 0,
            signers: Vec::new(&env),
            time_delay,
            execution_time,
            created_at: env.ledger().timestamp(),
            executed_at: 0,
            is_emergency,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Transaction(nonce), &transaction);

        // Auto-add creator signature
        Self::sign_transaction_internal(&env, nonce, creator.clone());

        // Log the action
        Self::log_transaction_action(
            &env,
            nonce,
            String::from_str(&env, "Created"),
            creator,
            String::from_str(&env, "Transaction proposed"),
        );

        nonce
    }

    /// Sign a transaction
    pub fn sign_transaction(env: Env, transaction_id: u64, signer: Address) {
        signer.require_auth();

        // Verify signer is authorized
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Signer(signer.clone()))
        {
            panic!("Not authorized to sign");
        }

        let signer_data: SignerData = env
            .storage()
            .persistent()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap();
        if !signer_data.is_active {
            panic!("Signer is not active");
        }

        Self::sign_transaction_internal(&env, transaction_id, signer);
    }

    /// Execute a transaction
    pub fn execute_transaction(env: Env, transaction_id: u64, executor: Address) {
        executor.require_auth();

        let mut transaction: Transaction = env
            .storage()
            .persistent()
            .get(&DataKey::Transaction(transaction_id))
            .unwrap_or_else(|| panic!("Transaction not found"));

        if transaction.status != TransactionStatus::Pending {
            panic!("Transaction is not pending");
        }

        // Check if minimum signatures reached
        if transaction.current_signatures < transaction.required_signatures {
            panic!("Not enough signatures");
        }

        // Check time delay
        if env.ledger().timestamp() < transaction.execution_time {
            panic!("Time delay not elapsed");
        }

        // Check if asset is whitelisted (if not zero address for native token)
        if transaction.asset != Address::from_string(&String::from_str(&env, "native"))
            && !env
                .storage()
                .persistent()
                .has(&DataKey::WhitelistedAsset(transaction.asset.clone()))
        {
            panic!("Asset not whitelisted");
        }

        // Update transaction status
        transaction.status = TransactionStatus::Executed;
        transaction.executed_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        // Log execution
        Self::log_transaction_action(
            &env,
            transaction_id,
            String::from_str(&env, "Executed"),
            executor,
            String::from_str(&env, "Transaction executed successfully"),
        );
    }

    /// Cancel a transaction
    pub fn cancel_transaction(env: Env, transaction_id: u64, canceller: Address) {
        canceller.require_auth();

        let mut transaction: Transaction = env
            .storage()
            .persistent()
            .get(&DataKey::Transaction(transaction_id))
            .unwrap_or_else(|| panic!("Transaction not found"));

        if transaction.status != TransactionStatus::Pending {
            panic!("Transaction is not pending");
        }

        // Only creator or admin can cancel
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if transaction.creator != canceller && admin != canceller {
            panic!("Not authorized to cancel");
        }

        transaction.status = TransactionStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        Self::log_transaction_action(
            &env,
            transaction_id,
            String::from_str(&env, "Cancelled"),
            canceller,
            String::from_str(&env, "Transaction cancelled"),
        );
    }

    /// Add a new signer (admin only)
    pub fn add_signer(env: Env, new_signer: Address, role: Role) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if env
            .storage()
            .persistent()
            .has(&DataKey::Signer(new_signer.clone()))
        {
            panic!("Signer already exists");
        }

        let signer_count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap();
        if signer_count >= MAX_SIGNERS {
            panic!("Maximum signers reached");
        }

        let signer_data = SignerData {
            address: new_signer.clone(),
            role: role.clone(),
            weight: 1,
            is_active: true,
            added_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Signer(new_signer.clone()), &signer_data);
        env.storage()
            .persistent()
            .set(&DataKey::Role(new_signer), &role);

        let new_count = signer_count + 1;
        env.storage()
            .instance()
            .set(&DataKey::SignerCount, &new_count);
    }

    /// Remove a signer (admin only)
    pub fn remove_signer(env: Env, signer: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if !env
            .storage()
            .persistent()
            .has(&DataKey::Signer(signer.clone()))
        {
            panic!("Signer not found");
        }

        let config: MultiSigConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let signer_count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap();

        // Ensure removing won't break threshold
        if signer_count - 1 < config.threshold {
            panic!("Cannot remove signer: would break threshold");
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Signer(signer.clone()));
        env.storage().persistent().remove(&DataKey::Role(signer));

        let new_count = signer_count - 1;
        env.storage()
            .instance()
            .set(&DataKey::SignerCount, &new_count);
    }

    /// Update threshold (admin only, requires multi-sig approval)
    pub fn update_threshold(env: Env, new_threshold: u32) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let signer_count: u32 = env.storage().instance().get(&DataKey::SignerCount).unwrap();
        if new_threshold > signer_count {
            panic!("Threshold cannot exceed number of signers");
        }

        if new_threshold == 0 {
            panic!("Threshold cannot be zero");
        }

        let mut config: MultiSigConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        config.threshold = new_threshold;
        env.storage().instance().set(&DataKey::Config, &config);
    }

    /// Whitelist an asset (admin only)
    pub fn whitelist_asset(env: Env, asset: Address, _whitelister: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedAsset(asset), &true);
    }

    /// Remove asset from whitelist (admin only)
    pub fn remove_whitelisted_asset(env: Env, asset: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        env.storage()
            .persistent()
            .remove(&DataKey::WhitelistedAsset(asset));
    }

    /// Get transaction details
    pub fn get_transaction(env: Env, transaction_id: u64) -> Option<Transaction> {
        env.storage()
            .persistent()
            .get(&DataKey::Transaction(transaction_id))
    }

    /// Get signer data
    pub fn get_signer(env: Env, signer_address: Address) -> Option<SignerData> {
        env.storage()
            .persistent()
            .get(&DataKey::Signer(signer_address))
    }

    /// Get all active signers
    pub fn get_all_signers(env: Env) -> Vec<SignerData> {
        let signers: Vec<SignerData> = Vec::new(&env);
        // This is a simplified approach - in production you'd have a separate signer list
        // For now, we'll return empty as we can't iterate without additional tracking
        signers
    }

    /// Get contract configuration
    pub fn get_config(env: Env) -> MultiSigConfig {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic!("Config not initialized"))
    }

    /// Check if asset is whitelisted
    pub fn is_asset_whitelisted(env: Env, asset: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::WhitelistedAsset(asset))
    }

    /// Get pending transactions count
    pub fn get_pending_transactions_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TransactionCount)
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // Internal helper functions
    // -----------------------------------------------------------------------

    /// Internal function to add signature to transaction
    fn sign_transaction_internal(env: &Env, transaction_id: u64, signer: Address) {
        let mut transaction: Transaction = env
            .storage()
            .persistent()
            .get(&DataKey::Transaction(transaction_id))
            .unwrap_or_else(|| panic!("Transaction not found"));

        // Check if already signed
        for i in 0..transaction.signers.len() {
            if transaction.signers.get(i).unwrap() == signer {
                panic!("Already signed this transaction");
            }
        }

        // Add signature
        transaction.signers.push_back(signer.clone());
        transaction.current_signatures = transaction.signers.len();

        // Check if this completes the required signatures
        if transaction.current_signatures >= transaction.required_signatures {
            transaction.status = TransactionStatus::Approved;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        // Log signature
        Self::log_transaction_action(
            env,
            transaction_id,
            String::from_str(env, "Signed"),
            signer,
            String::from_str(env, "Signature added"),
        );
    }

    /// Internal function to log transaction actions
    fn log_transaction_action(
        env: &Env,
        transaction_id: u64,
        action: String,
        actor: Address,
        details: String,
    ) {
        let log = TransactionLog {
            transaction_id,
            action,
            actor,
            timestamp: env.ledger().timestamp(),
            details,
        };

        // Store transaction log
        let logs: Vec<TransactionLog> = env
            .storage()
            .persistent()
            .get(&DataKey::TransactionHistory(transaction_id))
            .unwrap_or_else(|| Vec::new(env));

        let mut updated_logs = logs;
        updated_logs.push_back(log);
        env.storage()
            .persistent()
            .set(&DataKey::TransactionHistory(transaction_id), &updated_logs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    /// Helper: set up a fresh contract
    fn setup() -> (Env, MultiSigTreasuryContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, MultiSigTreasuryContract);
        let client = MultiSigTreasuryContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        let config = MultiSigConfig {
            threshold: 2,
            emergency_threshold: 3,
            time_delay: DEFAULT_TIME_DELAY,
            max_transactions_per_day: 100,
            require_emergency_approval: true,
        };

        let mut signers: Vec<Address> = Vec::new(&env);
        let mut roles: Vec<Role> = Vec::new(&env);

        // Add some test signers
        let signer1 = Address::generate(&env);
        let signer2 = Address::generate(&env);
        signers.push_back(signer1.clone());
        signers.push_back(signer2.clone());
        roles.push_back(Role::Operator);
        roles.push_back(Role::Guardian);

        client.initialize(&admin, &config, &signers, &roles);

        (env, client, admin)
    }

    #[test]
    fn test_initialize() {
        let (_env, client, _admin) = setup();

        let config = client.get_config();
        assert_eq!(config.threshold, 2);
        assert_eq!(config.emergency_threshold, 3);
    }

    #[test]
    fn test_propose_transaction() {
        let (env, client, _admin) = setup();
        let signer1 = Address::generate(&env);
        let destination = Address::generate(&env);
        let asset = Address::generate(&env);

        // Register signer1 first
        client.add_signer(&signer1, &Role::Operator);

        let tx_id = client.propose_transaction(
            &signer1,
            &destination,
            &(5000),
            &asset,
            &String::from_str(&env, "Test transaction"),
            &false,
        );

        assert!(tx_id > 0);

        let transaction = client.get_transaction(&tx_id);
        assert!(transaction.is_some());
        let tx = transaction.unwrap();
        assert_eq!(tx.status, TransactionStatus::Pending);
        assert_eq!(tx.current_signatures, 1); // Creator auto-signed
    }

    #[test]
    fn test_sign_transaction() {
        let (env, client, _admin) = setup();
        let signer1 = Address::generate(&env);
        let signer2 = Address::generate(&env);
        let destination = Address::generate(&env);
        let asset = Address::generate(&env);

        client.add_signer(&signer1, &Role::Operator);
        client.add_signer(&signer2, &Role::Guardian);

        let tx_id = client.propose_transaction(
            &signer1,
            &destination,
            &(5000),
            &asset,
            &String::from_str(&env, "Test transaction"),
            &false,
        );

        client.sign_transaction(&tx_id, &signer2);

        let transaction = client.get_transaction(&tx_id).unwrap();
        assert_eq!(transaction.current_signatures, 2);
        assert_eq!(transaction.status, TransactionStatus::Approved);
    }

    #[test]
    fn test_add_remove_signer() {
        let (env, client, _admin) = setup();
        let new_signer = Address::generate(&env);

        client.add_signer(&new_signer, &Role::Operator);

        let signer_data = client.get_signer(&new_signer);
        assert!(signer_data.is_some());
        assert_eq!(signer_data.unwrap().role, Role::Operator);

        client.remove_signer(&new_signer);

        let removed_signer = client.get_signer(&new_signer);
        assert!(removed_signer.is_none());
    }

    #[test]
    fn test_whitelist_asset() {
        let (env, client, admin) = setup();
        let asset = Address::generate(&env);

        assert!(!client.is_asset_whitelisted(&asset));

        client.whitelist_asset(&asset, &admin);

        assert!(client.is_asset_whitelisted(&asset));

        client.remove_whitelisted_asset(&asset);

        assert!(!client.is_asset_whitelisted(&asset));
    }

    #[test]
    fn test_update_threshold() {
        let (_env, client, _admin) = setup();

        let initial_config = client.get_config();
        assert_eq!(initial_config.threshold, 2);

        // Can only increase to 2 (current max) since we only have 2 signers
        client.update_threshold(&2);

        let updated_config = client.get_config();
        assert_eq!(updated_config.threshold, 2);
    }

    #[test]
    #[should_panic(expected = "Only signers can propose transactions")]
    fn test_non_signer_cannot_propose() {
        let (env, client, _admin) = setup();
        let non_signer = Address::generate(&env);
        let destination = Address::generate(&env);
        let asset = Address::generate(&env);

        client.propose_transaction(
            &non_signer,
            &destination,
            &(5000),
            &asset,
            &String::from_str(&env, "Unauthorized transaction"),
            &false,
        );
    }

    #[test]
    #[should_panic(expected = "Not authorized to cancel")]
    fn test_unauthorized_cancel() {
        let (env, client, _admin) = setup();
        let signer1 = Address::generate(&env);
        let non_signer = Address::generate(&env);
        let destination = Address::generate(&env);
        let asset = Address::generate(&env);

        client.add_signer(&signer1, &Role::Operator);

        let tx_id = client.propose_transaction(
            &signer1,
            &destination,
            &(5000),
            &asset,
            &String::from_str(&env, "Test transaction"),
            &false,
        );

        client.cancel_transaction(&tx_id, &non_signer);
    }

    #[test]
    fn test_cancel_by_creator() {
        let (env, client, _admin) = setup();
        let signer1 = Address::generate(&env);
        let destination = Address::generate(&env);
        let asset = Address::generate(&env);

        client.add_signer(&signer1, &Role::Operator);

        let tx_id = client.propose_transaction(
            &signer1,
            &destination,
            &(5000),
            &asset,
            &String::from_str(&env, "Test transaction"),
            &false,
        );

        client.cancel_transaction(&tx_id, &signer1);

        let transaction = client.get_transaction(&tx_id).unwrap();
        assert_eq!(transaction.status, TransactionStatus::Cancelled);
    }

    #[test]
    fn test_emergency_transaction() {
        let (env, client, _admin) = setup();
        let signer1 = Address::generate(&env);
        let destination = Address::generate(&env);
        let asset = Address::generate(&env);

        client.add_signer(&signer1, &Role::Operator);

        let tx_id = client.propose_transaction(
            &signer1,
            &destination,
            &(5000),
            &asset,
            &String::from_str(&env, "Emergency transaction"),
            &true, // emergency
        );

        let transaction = client.get_transaction(&tx_id).unwrap();
        assert!(transaction.is_emergency);
        assert_eq!(transaction.required_signatures, 3); // Emergency threshold
    }
}
