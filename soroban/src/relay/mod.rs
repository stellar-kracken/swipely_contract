//! Cross-chain message relay contract for Stellar Soroban.
//!
//! Features:
//! - Multi-chain adapters (Ethereum, Polygon, Base)
//! - Priority queue for pending messages
//! - Nonce tracking for replay protection
//! - Relay operator whitelist
//! - State proof verification (light-client style root checks)
//! - Batch relay execution
//! - Message expiry and cleanup
//! - Event emission for full message lifecycle

use soroban_sdk::{
    contract, contractimpl, panic_with_error, Address, Bytes, BytesN, Env, Map, Vec,
};

mod errors;
mod events;
mod types;

pub use errors::RelayError;
pub use types::{
    BatchRelayItem, ChainConfig, ChainId, CrossChainMessage, MessagePriority, MessageStatus,
    RelayDataKey, RelayOperator, RelayResult, StateProof,
};

#[contract]
pub struct CrossChainRelayContract;

#[contractimpl]
impl CrossChainRelayContract {
    // ---------------------------------------------------------------------
    // Admin / setup
    // ---------------------------------------------------------------------

    /// Initialize relay contract.
    pub fn initialize(env: Env, admin: Address, default_ttl: u64) -> Result<(), RelayError> {
        if default_ttl == 0 {
            return Err(RelayError::InvalidTtl);
        }

        if env.storage().instance().has(&RelayDataKey::Initialized) {
            return Err(RelayError::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().instance().set(&RelayDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&RelayDataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&RelayDataKey::DefaultTtl, &default_ttl);

        let queue: Vec<BytesN<32>> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&RelayDataKey::MessageQueue, &queue);

        let operators: Vec<Address> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&RelayDataKey::OperatorList, &operators);

        env.storage()
            .persistent()
            .set(&RelayDataKey::TotalMessages, &0u64);
        env.storage()
            .persistent()
            .set(&RelayDataKey::TotalRelayed, &0u64);
        env.storage()
            .persistent()
            .set(&RelayDataKey::TotalFees, &0i128);

        // Default chain configurations.
        set_chain_config_internal(
            &env,
            ChainConfig {
                chain_id: ChainId::Ethereum,
                base_fee: 1_500,
                fee_per_byte: 6,
                is_enabled: true,
            },
        );
        set_chain_config_internal(
            &env,
            ChainConfig {
                chain_id: ChainId::Polygon,
                base_fee: 800,
                fee_per_byte: 3,
                is_enabled: true,
            },
        );
        set_chain_config_internal(
            &env,
            ChainConfig {
                chain_id: ChainId::Base,
                base_fee: 1_000,
                fee_per_byte: 4,
                is_enabled: true,
            },
        );

        Ok(())
    }

    /// Configure fee model and status for a target chain (admin only).
    pub fn configure_chain(env: Env, config: ChainConfig) -> Result<(), RelayError> {
        require_admin(&env)?;
        set_chain_config_internal(&env, config);
        Ok(())
    }

    /// Register and whitelist a relay operator (admin only).
    pub fn register_relay_operator(
        env: Env,
        operator: Address,
        public_key: BytesN<32>,
    ) -> Result<(), RelayError> {
        require_admin(&env)?;

        if env
            .storage()
            .persistent()
            .has(&RelayDataKey::Operator(operator.clone()))
        {
            return Err(RelayError::OperatorAlreadyRegistered);
        }

        let relay_operator = RelayOperator {
            operator: operator.clone(),
            public_key,
            is_active: true,
            messages_relayed: 0,
            registered_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&RelayDataKey::Operator(operator.clone()), &relay_operator);

        let mut operator_list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&RelayDataKey::OperatorList)
            .unwrap_or(Vec::new(&env));
        operator_list.push_back(operator.clone());
        env.storage()
            .persistent()
            .set(&RelayDataKey::OperatorList, &operator_list);

        events::emit_operator_registered(&env, &operator);
        Ok(())
    }

    /// Deactivate a relay operator (admin only).
    pub fn deactivate_relay_operator(env: Env, operator: Address) -> Result<(), RelayError> {
        require_admin(&env)?;

        let mut relay_operator: RelayOperator = env
            .storage()
            .persistent()
            .get(&RelayDataKey::Operator(operator.clone()))
            .ok_or(RelayError::OperatorNotActive)?;

        relay_operator.is_active = false;

        env.storage()
            .persistent()
            .set(&RelayDataKey::Operator(operator.clone()), &relay_operator);

        events::emit_operator_deactivated(&env, &operator);
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Message flow
    // ---------------------------------------------------------------------

    /// Send a cross-chain message into the relay queue.
    ///
    /// `fee_paid` must be greater than or equal to the estimated relay fee.
    /// `ttl_override` when set to 0 means use default TTL.
    pub fn send_message(
        env: Env,
        source_chain: ChainId,
        dest_chain: ChainId,
        sender: Address,
        payload: Bytes,
        nonce: u64,
        priority: MessagePriority,
        ttl_override: u64,
        fee_paid: i128,
    ) -> Result<BytesN<32>, RelayError> {
        require_initialized(&env)?;
        sender.require_auth();

        if payload.len() > 16_384 {
            return Err(RelayError::PayloadTooLarge);
        }

        let chain_config = get_chain_config(&env, &dest_chain)?;
        if !chain_config.is_enabled {
            return Err(RelayError::ChainNotEnabled);
        }

        let expected_nonce = get_nonce(&env, &sender);
        if nonce != expected_nonce {
            return Err(RelayError::InvalidNonce);
        }

        let estimated_fee = estimate_fee_internal(&payload, &chain_config);
        if fee_paid < estimated_fee {
            return Err(RelayError::InsufficientFee);
        }

        let now = env.ledger().timestamp();
        let default_ttl: u64 = env
            .storage()
            .instance()
            .get(&RelayDataKey::DefaultTtl)
            .ok_or(RelayError::NotInitialized)?;
        let ttl = if ttl_override == 0 {
            default_ttl
        } else {
            ttl_override
        };

        if ttl == 0 {
            return Err(RelayError::InvalidTtl);
        }

        let expiry = now.saturating_add(ttl);
        let message_id = hash_message(
            &env,
            &source_chain,
            &dest_chain,
            &sender,
            &payload,
            nonce,
            now,
        );

        let message = CrossChainMessage {
            message_id: message_id.clone(),
            source_chain,
            dest_chain: dest_chain.clone(),
            sender: sender.clone(),
            payload,
            nonce,
            timestamp: now,
            expiry,
            priority,
            status: MessageStatus::Pending,
            fee: estimated_fee,
        };

        env.storage()
            .persistent()
            .set(&RelayDataKey::Message(message_id.clone()), &message);

        insert_queue_by_priority(&env, &message_id)?;

        set_nonce(&env, &sender, expected_nonce.saturating_add(1));

        increment_u64(&env, RelayDataKey::TotalMessages, 1);
        increment_i128(&env, RelayDataKey::TotalFees, estimated_fee);

        events::emit_message_sent(&env, &message_id, &sender, &dest_chain);
        Ok(message_id)
    }

    /// Verify message and associated source-chain state proof.
    pub fn verify_message(
        env: Env,
        message_id: BytesN<32>,
        proof: StateProof,
    ) -> Result<bool, RelayError> {
        require_initialized(&env)?;

        let mut message = get_message_or_err(&env, &message_id)?;

        if is_expired(&env, &message) {
            let old_status = message.status.clone();
            message.status = MessageStatus::Expired;
            env.storage()
                .persistent()
                .set(&RelayDataKey::Message(message_id.clone()), &message);
            events::emit_message_status_changed(
                &env,
                &message_id,
                &old_status,
                &MessageStatus::Expired,
            );
            return Err(RelayError::MessageExpired);
        }

        if message.status != MessageStatus::Pending {
            return Err(RelayError::InvalidMessageStatus);
        }

        if proof.chain_id != message.source_chain {
            return Err(RelayError::InvalidStateProof);
        }

        let proof_ok = verify_state_proof_internal(&env, &proof)?;
        if !proof_ok {
            return Err(RelayError::InvalidStateProof);
        }

        let old_status = message.status.clone();
        message.status = MessageStatus::Verified;
        env.storage()
            .persistent()
            .set(&RelayDataKey::Message(message_id.clone()), &message);

        events::emit_message_status_changed(
            &env,
            &message_id,
            &old_status,
            &MessageStatus::Verified,
        );

        Ok(true)
    }

    /// Relay a single verified/pending message by an active operator.
    ///
    /// Signature format:
    /// `signature[0..32] == sha256(message_id || operator_public_key)` and
    /// `signature[32..64] == sha256(message_id || operator_public_key)`.
    pub fn relay_message(
        env: Env,
        operator: Address,
        message_id: BytesN<32>,
        signature: BytesN<64>,
    ) -> Result<bool, RelayError> {
        require_initialized(&env)?;
        operator.require_auth();
        relay_message_internal(&env, &operator, &message_id, &signature)
    }

    /// Verify source chain state proof.
    pub fn verify_state_proof(env: Env, proof: StateProof) -> Result<bool, RelayError> {
        require_initialized(&env)?;
        verify_state_proof_internal(&env, &proof)
    }

    /// Relay a batch of messages for gas/cost efficiency.
    pub fn batch_relay(
        env: Env,
        operator: Address,
        items: Vec<BatchRelayItem>,
    ) -> Result<RelayResult, RelayError> {
        require_initialized(&env)?;
        operator.require_auth();

        if items.len() == 0 {
            return Err(RelayError::EmptyBatch);
        }

        let mut relayed_ids: Vec<BytesN<32>> = Vec::new(&env);
        let mut success_count: u32 = 0;
        let mut failure_count: u32 = 0;

        let mut i: u32 = 0;
        while i < items.len() {
            let item = items.get(i).unwrap();
            let result = relay_message_internal(&env, &operator, &item.message_id, &item.signature);

            if result.is_ok() {
                success_count = success_count.saturating_add(1);
                relayed_ids.push_back(item.message_id);
            } else {
                failure_count = failure_count.saturating_add(1);
            }

            i = i.saturating_add(1);
        }

        events::emit_batch_relayed(&env, success_count, failure_count);

        Ok(RelayResult {
            success_count,
            failure_count,
            relayed_ids,
        })
    }

    // ---------------------------------------------------------------------
    // Utilities / views
    // ---------------------------------------------------------------------

    /// Return current sender nonce.
    pub fn get_sender_nonce(env: Env, sender: Address) -> u64 {
        get_nonce(&env, &sender)
    }

    /// Return current message by id.
    pub fn get_message(env: Env, message_id: BytesN<32>) -> Option<CrossChainMessage> {
        env.storage()
            .persistent()
            .get(&RelayDataKey::Message(message_id))
    }

    /// Return relay operator metadata by address.
    pub fn get_operator(env: Env, operator: Address) -> Option<RelayOperator> {
        env.storage()
            .persistent()
            .get(&RelayDataKey::Operator(operator))
    }

    /// Return current pending message queue.
    pub fn get_message_queue(env: Env) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&RelayDataKey::MessageQueue)
            .unwrap_or(Vec::new(&env))
    }

    /// Return next message in queue (highest priority).
    pub fn peek_next_message(env: Env) -> Option<BytesN<32>> {
        let queue: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&RelayDataKey::MessageQueue)
            .unwrap_or(Vec::new(&env));

        queue.get(0)
    }

    /// Estimate relay fee for a payload and destination chain.
    pub fn estimate_fee(env: Env, dest_chain: ChainId, payload: Bytes) -> Result<i128, RelayError> {
        let config = get_chain_config(&env, &dest_chain)?;
        if !config.is_enabled {
            return Err(RelayError::ChainNotEnabled);
        }
        Ok(estimate_fee_internal(&payload, &config))
    }

    /// Cleanup expired pending messages from queue.
    pub fn cleanup_expired_messages(env: Env, max_items: u32) -> Result<u32, RelayError> {
        require_initialized(&env)?;

        if max_items == 0 {
            return Ok(0);
        }

        let mut queue: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&RelayDataKey::MessageQueue)
            .unwrap_or(Vec::new(&env));

        let mut cleaned: u32 = 0;
        let mut scanned: u32 = 0;

        while scanned < queue.len() && scanned < max_items {
            let id = queue.get(scanned).unwrap();
            let mut remove = false;

            if let Some(mut msg) = env
                .storage()
                .persistent()
                .get::<RelayDataKey, CrossChainMessage>(&RelayDataKey::Message(id.clone()))
            {
                if is_expired(&env, &msg)
                    && (msg.status == MessageStatus::Pending
                        || msg.status == MessageStatus::Verified)
                {
                    let old = msg.status.clone();
                    msg.status = MessageStatus::Expired;
                    env.storage()
                        .persistent()
                        .set(&RelayDataKey::Message(id.clone()), &msg);
                    events::emit_message_status_changed(&env, &id, &old, &MessageStatus::Expired);
                    remove = true;
                } else if msg.status == MessageStatus::Relayed
                    || msg.status == MessageStatus::Expired
                {
                    remove = true;
                }
            } else {
                remove = true;
            }

            if remove {
                queue.remove(scanned);
                cleaned = cleaned.saturating_add(1);
            } else {
                scanned = scanned.saturating_add(1);
            }
        }

        env.storage()
            .persistent()
            .set(&RelayDataKey::MessageQueue, &queue);

        if cleaned > 0 {
            events::emit_messages_cleaned(&env, cleaned);
        }

        Ok(cleaned)
    }

    /// Return aggregated relay metrics.
    pub fn get_metrics(env: Env) -> Map<Bytes, i128> {
        let mut metrics = Map::new(&env);

        let total_messages: u64 = env
            .storage()
            .persistent()
            .get(&RelayDataKey::TotalMessages)
            .unwrap_or(0);
        let total_relayed: u64 = env
            .storage()
            .persistent()
            .get(&RelayDataKey::TotalRelayed)
            .unwrap_or(0);
        let total_fees: i128 = env
            .storage()
            .persistent()
            .get(&RelayDataKey::TotalFees)
            .unwrap_or(0);

        metrics.set(bytes_key(&env, b"total_messages"), total_messages as i128);
        metrics.set(bytes_key(&env, b"total_relayed"), total_relayed as i128);
        metrics.set(bytes_key(&env, b"total_fees"), total_fees);

        metrics
    }
}

// -------------------------------------------------------------------------
// Internal helpers
// -------------------------------------------------------------------------

fn require_initialized(env: &Env) -> Result<(), RelayError> {
    if !env.storage().instance().has(&RelayDataKey::Initialized) {
        return Err(RelayError::NotInitialized);
    }
    Ok(())
}

fn require_admin(env: &Env) -> Result<(), RelayError> {
    require_initialized(env)?;
    let admin: Address = env
        .storage()
        .instance()
        .get(&RelayDataKey::Admin)
        .ok_or(RelayError::NotInitialized)?;
    admin.require_auth();
    Ok(())
}

fn chain_to_u32(chain: &ChainId) -> u32 {
    match chain {
        ChainId::Stellar => 0,
        ChainId::Ethereum => 1,
        ChainId::Polygon => 2,
        ChainId::Base => 3,
    }
}

fn set_chain_config_internal(env: &Env, config: ChainConfig) {
    let key = RelayDataKey::ChainConfig(chain_to_u32(&config.chain_id));
    env.storage().persistent().set(&key, &config);
}

fn get_chain_config(env: &Env, chain: &ChainId) -> Result<ChainConfig, RelayError> {
    let key = RelayDataKey::ChainConfig(chain_to_u32(chain));
    env.storage()
        .persistent()
        .get(&key)
        .ok_or(RelayError::ChainConfigNotFound)
}

fn get_nonce(env: &Env, sender: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&RelayDataKey::Nonce(sender.clone()))
        .unwrap_or(0)
}

fn set_nonce(env: &Env, sender: &Address, nonce: u64) {
    env.storage()
        .persistent()
        .set(&RelayDataKey::Nonce(sender.clone()), &nonce);
}

fn estimate_fee_internal(payload: &Bytes, cfg: &ChainConfig) -> i128 {
    cfg.base_fee
        .saturating_add(cfg.fee_per_byte.saturating_mul(payload.len() as i128))
}

fn is_expired(env: &Env, message: &CrossChainMessage) -> bool {
    env.ledger().timestamp() >= message.expiry
}

fn bytes_key(env: &Env, text: &[u8]) -> Bytes {
    Bytes::from_slice(env, text)
}

fn append_u32(buf: &mut Bytes, v: u32) {
    let b = v.to_be_bytes();
    let mut i = 0;
    while i < b.len() {
        buf.push_back(b[i]);
        i += 1;
    }
}

fn append_u64(buf: &mut Bytes, v: u64) {
    let b = v.to_be_bytes();
    let mut i = 0;
    while i < b.len() {
        buf.push_back(b[i]);
        i += 1;
    }
}

fn append_bytesn<const N: usize>(buf: &mut Bytes, b: &BytesN<N>) {
    let bytes = b.to_array();
    let mut i = 0;
    while i < bytes.len() {
        buf.push_back(bytes[i]);
        i += 1;
    }
}

fn hash_message(
    env: &Env,
    source_chain: &ChainId,
    dest_chain: &ChainId,
    sender: &Address,
    payload: &Bytes,
    nonce: u64,
    timestamp: u64,
) -> BytesN<32> {
    let mut data = Bytes::new(env);
    append_u32(&mut data, chain_to_u32(source_chain));
    append_u32(&mut data, chain_to_u32(dest_chain));
    append_u64(&mut data, nonce);
    append_u64(&mut data, timestamp);
    // Incorporate sender identity by hashing their string representation.
    let sender_str = sender.to_string();
    let len = sender_str.len() as usize;
    let mut buf = [0u8; 56]; // Stellar addresses are max 56 chars
    let slice = &mut buf[..len];
    sender_str.copy_into_slice(slice);
    let mut j = 0usize;
    while j < len {
        data.push_back(slice[j]);
        j += 1;
    }
    data.append(payload);

    env.crypto().sha256(&data).into()
}

fn get_message_or_err(env: &Env, id: &BytesN<32>) -> Result<CrossChainMessage, RelayError> {
    env.storage()
        .persistent()
        .get(&RelayDataKey::Message(id.clone()))
        .ok_or(RelayError::MessageNotFound)
}

fn get_operator_or_err(env: &Env, operator: &Address) -> Result<RelayOperator, RelayError> {
    env.storage()
        .persistent()
        .get(&RelayDataKey::Operator(operator.clone()))
        .ok_or(RelayError::OperatorNotActive)
}

fn increment_u64(env: &Env, key: RelayDataKey, amount: u64) {
    let current: u64 = env.storage().persistent().get(&key).unwrap_or(0);
    env.storage()
        .persistent()
        .set(&key, &current.saturating_add(amount));
}

fn increment_i128(env: &Env, key: RelayDataKey, amount: i128) {
    let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    env.storage()
        .persistent()
        .set(&key, &current.saturating_add(amount));
}

fn message_priority_value(priority: &MessagePriority) -> u32 {
    match priority {
        MessagePriority::Critical => 0,
        MessagePriority::High => 1,
        MessagePriority::Medium => 2,
        MessagePriority::Low => 3,
    }
}

/// Internal relay logic (no auth check – callers must verify auth).
fn relay_message_internal(
    env: &Env,
    operator: &Address,
    message_id: &BytesN<32>,
    signature: &BytesN<64>,
) -> Result<bool, RelayError> {
    let mut relay_operator = get_operator_or_err(env, operator)?;
    if !relay_operator.is_active {
        return Err(RelayError::OperatorNotActive);
    }

    let mut message = get_message_or_err(env, message_id)?;

    if is_expired(env, &message) {
        let old_status = message.status.clone();
        message.status = MessageStatus::Expired;
        env.storage()
            .persistent()
            .set(&RelayDataKey::Message(message_id.clone()), &message);
        events::emit_message_status_changed(env, message_id, &old_status, &MessageStatus::Expired);
        return Err(RelayError::MessageExpired);
    }

    if message.status != MessageStatus::Pending && message.status != MessageStatus::Verified {
        return Err(RelayError::InvalidMessageStatus);
    }

    verify_operator_signature(env, message_id, &relay_operator.public_key, signature)?;

    let old_status = message.status.clone();
    message.status = MessageStatus::Relayed;
    env.storage()
        .persistent()
        .set(&RelayDataKey::Message(message_id.clone()), &message);

    relay_operator.messages_relayed = relay_operator.messages_relayed.saturating_add(1);
    env.storage()
        .persistent()
        .set(&RelayDataKey::Operator(operator.clone()), &relay_operator);

    remove_from_queue(env, message_id);
    increment_u64(env, RelayDataKey::TotalRelayed, 1);

    events::emit_message_status_changed(env, message_id, &old_status, &MessageStatus::Relayed);
    events::emit_message_relayed(env, message_id, operator);

    Ok(true)
}

fn insert_queue_by_priority(env: &Env, message_id: &BytesN<32>) -> Result<(), RelayError> {
    let mut queue: Vec<BytesN<32>> = env
        .storage()
        .persistent()
        .get(&RelayDataKey::MessageQueue)
        .unwrap_or(Vec::new(env));

    let message = get_message_or_err(env, message_id)?;
    let target_priority = message_priority_value(&message.priority);

    let mut insert_at = queue.len();
    let mut i: u32 = 0;
    while i < queue.len() {
        let qid = queue.get(i).unwrap();
        if let Some(qmsg) = env
            .storage()
            .persistent()
            .get::<RelayDataKey, CrossChainMessage>(&RelayDataKey::Message(qid))
        {
            let qpri = message_priority_value(&qmsg.priority);
            if target_priority < qpri {
                insert_at = i;
                break;
            }
        }
        i = i.saturating_add(1);
    }

    queue.insert(insert_at, message_id.clone());
    env.storage()
        .persistent()
        .set(&RelayDataKey::MessageQueue, &queue);

    Ok(())
}

fn remove_from_queue(env: &Env, message_id: &BytesN<32>) {
    let mut queue: Vec<BytesN<32>> = env
        .storage()
        .persistent()
        .get(&RelayDataKey::MessageQueue)
        .unwrap_or(Vec::new(env));

    let mut i: u32 = 0;
    while i < queue.len() {
        if queue.get(i).unwrap() == *message_id {
            queue.remove(i);
            break;
        }
        i = i.saturating_add(1);
    }

    env.storage()
        .persistent()
        .set(&RelayDataKey::MessageQueue, &queue);
}

fn verify_operator_signature(
    env: &Env,
    message_id: &BytesN<32>,
    public_key: &BytesN<32>,
    signature: &BytesN<64>,
) -> Result<(), RelayError> {
    let mut data = Bytes::new(env);
    append_bytesn(&mut data, message_id);
    append_bytesn(&mut data, public_key);
    let digest: BytesN<32> = env.crypto().sha256(&data).into();

    let digest_arr = digest.to_array();
    let sig_arr = signature.to_array();

    let mut i = 0usize;
    while i < 32 {
        if sig_arr[i] != digest_arr[i] || sig_arr[i + 32] != digest_arr[i] {
            return Err(RelayError::InvalidSignature);
        }
        i += 1;
    }

    Ok(())
}

fn verify_state_proof_internal(env: &Env, proof: &StateProof) -> Result<bool, RelayError> {
    let config = get_chain_config(env, &proof.chain_id)?;
    if !config.is_enabled {
        return Err(RelayError::ChainNotEnabled);
    }

    // Light-client style check #1: proof blob must hash to claimed state root.
    let computed_root: BytesN<32> = env.crypto().sha256(&proof.proof_data).into();
    if computed_root != proof.state_root {
        return Err(RelayError::InvalidStateProof);
    }

    // Check validator is an active relay operator.
    let operator_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&RelayDataKey::OperatorList)
        .unwrap_or(Vec::new(env));

    let mut found_active = false;
    let mut i: u32 = 0;
    while i < operator_list.len() {
        let op_addr = operator_list.get(i).unwrap();
        if let Some(op) = env
            .storage()
            .persistent()
            .get::<RelayDataKey, RelayOperator>(&RelayDataKey::Operator(op_addr))
        {
            if op.is_active && op.public_key == proof.validator_key {
                found_active = true;
                break;
            }
        }

        i = i.saturating_add(1);
    }

    if !found_active {
        return Err(RelayError::OperatorNotActive);
    }

    // Signature check over attestation material.
    let mut attestation = Bytes::new(env);
    append_u32(&mut attestation, chain_to_u32(&proof.chain_id));
    append_u64(&mut attestation, proof.block_number);
    append_bytesn(&mut attestation, &proof.state_root);
    append_bytesn(&mut attestation, &proof.validator_key);

    let digest: BytesN<32> = env.crypto().sha256(&attestation).into();
    let digest_arr = digest.to_array();
    let sig_arr = proof.signature.to_array();

    let mut j = 0usize;
    while j < 32 {
        if sig_arr[j] != digest_arr[j] || sig_arr[j + 32] != digest_arr[j] {
            return Err(RelayError::InvalidSignature);
        }
        j += 1;
    }

    events::emit_state_proof_verified(env, &proof.chain_id, proof.block_number);
    Ok(true)
}

#[allow(dead_code)]
fn fail(env: &Env, err: RelayError) -> ! {
    panic_with_error!(env, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    fn setup(
        env: &Env,
        client: &CrossChainRelayContractClient,
        admin: &Address,
        operator: &Address,
    ) {
        client.initialize(admin, &300);
        let op_pk = BytesN::from_array(env, &[7u8; 32]);
        client.register_relay_operator(operator, &op_pk);
    }

    fn setup_context() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CrossChainRelayContract);
        let client = CrossChainRelayContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);

        setup(&env, &client, &admin, &operator);

        (env, contract_id, operator)
    }

    fn make_signature(env: &Env, message_id: &BytesN<32>, public_key: &BytesN<32>) -> BytesN<64> {
        let mut data = Bytes::new(env);
        append_bytesn(&mut data, message_id);
        append_bytesn(&mut data, public_key);
        let digest: BytesN<32> = env.crypto().sha256(&data).into();
        let d = digest.to_array();
        let mut out = [0u8; 64];
        let mut i = 0usize;
        while i < 32 {
            out[i] = d[i];
            out[i + 32] = d[i];
            i += 1;
        }
        BytesN::from_array(env, &out)
    }

    #[test]
    fn send_message_tracks_nonce_and_queue() {
        let (env, contract_id, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &contract_id);
        let sender = Address::generate(&env);
        let payload = Bytes::from_slice(&env, b"bridge-health:ok");

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &payload,
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        assert_eq!(client.get_sender_nonce(&sender), 1);
        let queued = client.peek_next_message();
        assert_eq!(queued, Some(msg_id));
    }

    #[test]
    fn message_priority_orders_queue() {
        let (env, contract_id, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &contract_id);
        let sender = Address::generate(&env);

        let low_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"low"),
            &0,
            &MessagePriority::Low,
            &0,
            &5_000,
        );

        let high_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"critical"),
            &1,
            &MessagePriority::Critical,
            &0,
            &5_000,
        );

        let queue = client.get_message_queue();
        assert_eq!(queue.get(0), Some(high_id));
        assert_eq!(queue.get(1), Some(low_id));
    }

    #[test]
    fn relay_message_changes_status() {
        let (env, contract_id, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &contract_id);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Base,
            &sender,
            &Bytes::from_slice(&env, b"batch me"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );

        let op = client.get_operator(&operator).unwrap();
        let sig = make_signature(&env, &msg_id, &op.public_key);

        let ok = client.relay_message(&operator, &msg_id, &sig);
        assert!(ok);

        let msg = client.get_message(&msg_id).unwrap();
        assert_eq!(msg.status, MessageStatus::Relayed);
    }

    #[test]
    fn batch_relay_returns_aggregate_result() {
        let (env, contract_id, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &contract_id);
        let sender = Address::generate(&env);

        let m1 = client.send_message(
            &ChainId::Stellar,
            &ChainId::Polygon,
            &sender,
            &Bytes::from_slice(&env, b"m1"),
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        let m2 = client.send_message(
            &ChainId::Stellar,
            &ChainId::Polygon,
            &sender,
            &Bytes::from_slice(&env, b"m2"),
            &1,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        let op = client.get_operator(&operator).unwrap();
        let s1 = make_signature(&env, &m1, &op.public_key);
        let s2 = make_signature(&env, &m2, &op.public_key);

        let mut items: Vec<BatchRelayItem> = Vec::new(&env);
        items.push_back(BatchRelayItem {
            message_id: m1,
            signature: s1,
        });
        items.push_back(BatchRelayItem {
            message_id: m2,
            signature: s2,
        });

        let result = client.batch_relay(&operator, &items);
        assert_eq!(result.success_count, 2);
        assert_eq!(result.failure_count, 0);
    }

    #[test]
    fn verify_state_proof_works() {
        let (env, contract_id, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &contract_id);

        let proof_data = Bytes::from_slice(&env, b"mock-merkle-proof");
        let state_root: BytesN<32> = env.crypto().sha256(&proof_data).into();
        let validator_key = BytesN::from_array(&env, &[7u8; 32]);

        let mut attestation = Bytes::new(&env);
        append_u32(&mut attestation, chain_to_u32(&ChainId::Ethereum));
        append_u64(&mut attestation, 123);
        append_bytesn(&mut attestation, &state_root);
        append_bytesn(&mut attestation, &validator_key);

        let digest: BytesN<32> = env.crypto().sha256(&attestation).into();
        let d = digest.to_array();
        let mut sig = [0u8; 64];
        let mut i = 0usize;
        while i < 32 {
            sig[i] = d[i];
            sig[i + 32] = d[i];
            i += 1;
        }

        let proof = StateProof {
            chain_id: ChainId::Ethereum,
            block_number: 123,
            state_root,
            proof_data,
            validator_key,
            signature: BytesN::from_array(&env, &sig),
        };

        assert!(client.verify_state_proof(&proof));
    }

    // ---------------------------------------------------------------
    // Initialization edge cases
    // ---------------------------------------------------------------

    #[test]
    fn test_double_initialize_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let cid = env.register_contract(None, CrossChainRelayContract);
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let admin = Address::generate(&env);

        client.initialize(&admin, &300);
        let result = client.try_initialize(&admin, &300);
        assert_eq!(result, Err(Ok(RelayError::AlreadyInitialized)));
    }

    #[test]
    fn test_initialize_zero_ttl_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let cid = env.register_contract(None, CrossChainRelayContract);
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let admin = Address::generate(&env);

        let result = client.try_initialize(&admin, &0);
        assert_eq!(result, Err(Ok(RelayError::InvalidTtl)));
    }

    // ---------------------------------------------------------------
    // Nonce and replay protection
    // ---------------------------------------------------------------

    #[test]
    fn test_invalid_nonce_rejected() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        // Nonce should start at 0, sending 1 should fail.
        let result = client.try_send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"test"),
            &1,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );
        assert_eq!(result, Err(Ok(RelayError::InvalidNonce)));
    }

    #[test]
    fn test_nonce_increments_sequentially() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        assert_eq!(client.get_sender_nonce(&sender), 0);

        client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"a"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );
        assert_eq!(client.get_sender_nonce(&sender), 1);

        client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"b"),
            &1,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );
        assert_eq!(client.get_sender_nonce(&sender), 2);
    }

    // ---------------------------------------------------------------
    // Fee estimation
    // ---------------------------------------------------------------

    #[test]
    fn test_estimate_fee() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let payload = Bytes::from_slice(&env, b"hello");
        // Ethereum: base=1500, per_byte=6, payload=5 bytes => 1500 + 30 = 1530
        let fee = client.estimate_fee(&ChainId::Ethereum, &payload);
        assert_eq!(fee, 1530);

        // Polygon: base=800, per_byte=3, payload=5 bytes => 800 + 15 = 815
        let fee = client.estimate_fee(&ChainId::Polygon, &payload);
        assert_eq!(fee, 815);
    }

    #[test]
    fn test_insufficient_fee_rejected() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let result = client.try_send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"test"),
            &0,
            &MessagePriority::Medium,
            &0,
            &1, // Too low
        );
        assert_eq!(result, Err(Ok(RelayError::InsufficientFee)));
    }

    // ---------------------------------------------------------------
    // Payload limits
    // ---------------------------------------------------------------

    #[test]
    fn test_payload_too_large() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        // Create a payload > 16384 bytes using from_slice
        let large = Bytes::from_slice(&env, &[0x41u8; 16_385]);

        let result = client.try_send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &large,
            &0,
            &MessagePriority::Medium,
            &0,
            &1_000_000,
        );
        assert_eq!(result, Err(Ok(RelayError::PayloadTooLarge)));
    }

    // ---------------------------------------------------------------
    // Chain config
    // ---------------------------------------------------------------

    #[test]
    fn test_configure_chain() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        // Reconfigure Ethereum fees
        client.configure_chain(&ChainConfig {
            chain_id: ChainId::Ethereum,
            base_fee: 3_000,
            fee_per_byte: 10,
            is_enabled: true,
        });

        let fee = client.estimate_fee(&ChainId::Ethereum, &Bytes::from_slice(&env, b"hi"));
        // base=3000, per_byte=10, 2 bytes => 3020
        assert_eq!(fee, 3020);
    }

    #[test]
    fn test_disabled_chain_rejected() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        // Disable Ethereum
        client.configure_chain(&ChainConfig {
            chain_id: ChainId::Ethereum,
            base_fee: 1_500,
            fee_per_byte: 6,
            is_enabled: false,
        });

        let result = client.try_send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"test"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );
        assert_eq!(result, Err(Ok(RelayError::ChainNotEnabled)));
    }

    // ---------------------------------------------------------------
    // Relay operator management
    // ---------------------------------------------------------------

    #[test]
    fn test_duplicate_operator_rejected() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let result =
            client.try_register_relay_operator(&operator, &BytesN::from_array(&env, &[7u8; 32]));
        assert_eq!(result, Err(Ok(RelayError::OperatorAlreadyRegistered)));
    }

    #[test]
    fn test_deactivate_operator() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        client.deactivate_relay_operator(&operator);
        let op = client.get_operator(&operator).unwrap();
        assert!(!op.is_active);
    }

    #[test]
    fn test_deactivated_operator_cannot_relay() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"test"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );

        client.deactivate_relay_operator(&operator);

        let op = client.get_operator(&operator).unwrap();
        let sig = make_signature(&env, &msg_id, &op.public_key);

        let result = client.try_relay_message(&operator, &msg_id, &sig);
        assert_eq!(result, Err(Ok(RelayError::OperatorNotActive)));
    }

    #[test]
    fn test_deactivate_unknown_operator_fails() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let unknown = Address::generate(&env);

        let result = client.try_deactivate_relay_operator(&unknown);
        assert_eq!(result, Err(Ok(RelayError::OperatorNotActive)));
    }

    // ---------------------------------------------------------------
    // Invalid signatures
    // ---------------------------------------------------------------

    #[test]
    fn test_bad_signature_rejected() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"test"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );

        let bad_sig = BytesN::from_array(&env, &[0u8; 64]);
        let result = client.try_relay_message(&operator, &msg_id, &bad_sig);
        assert_eq!(result, Err(Ok(RelayError::InvalidSignature)));
    }

    // ---------------------------------------------------------------
    // Relay already relayed message
    // ---------------------------------------------------------------

    #[test]
    fn test_relay_already_relayed_fails() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"test"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );

        let op = client.get_operator(&operator).unwrap();
        let sig = make_signature(&env, &msg_id, &op.public_key);

        client.relay_message(&operator, &msg_id, &sig);

        // Try to relay again
        let result = client.try_relay_message(&operator, &msg_id, &sig);
        assert_eq!(result, Err(Ok(RelayError::InvalidMessageStatus)));
    }

    // ---------------------------------------------------------------
    // Relay unknown message
    // ---------------------------------------------------------------

    #[test]
    fn test_relay_unknown_message_fails() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let fake_id = BytesN::from_array(&env, &[99u8; 32]);
        let fake_sig = BytesN::from_array(&env, &[0u8; 64]);

        let result = client.try_relay_message(&operator, &fake_id, &fake_sig);
        assert_eq!(result, Err(Ok(RelayError::MessageNotFound)));
    }

    // ---------------------------------------------------------------
    // Verify message (state proof based)
    // ---------------------------------------------------------------

    #[test]
    fn test_verify_message_changes_status() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Ethereum,
            &ChainId::Polygon,
            &sender,
            &Bytes::from_slice(&env, b"verify-me"),
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        // Build a valid state proof for Ethereum
        let proof_data = Bytes::from_slice(&env, b"eth-proof-data");
        let state_root: BytesN<32> = env.crypto().sha256(&proof_data).into();
        let validator_key = BytesN::from_array(&env, &[7u8; 32]);

        let mut attestation = Bytes::new(&env);
        append_u32(&mut attestation, chain_to_u32(&ChainId::Ethereum));
        append_u64(&mut attestation, 1000);
        append_bytesn(&mut attestation, &state_root);
        append_bytesn(&mut attestation, &validator_key);

        let digest: BytesN<32> = env.crypto().sha256(&attestation).into();
        let d = digest.to_array();
        let mut sig = [0u8; 64];
        let mut i = 0usize;
        while i < 32 {
            sig[i] = d[i];
            sig[i + 32] = d[i];
            i += 1;
        }

        let proof = StateProof {
            chain_id: ChainId::Ethereum,
            block_number: 1000,
            state_root,
            proof_data,
            validator_key,
            signature: BytesN::from_array(&env, &sig),
        };

        let ok = client.verify_message(&msg_id, &proof);
        assert!(ok);

        let msg = client.get_message(&msg_id).unwrap();
        assert_eq!(msg.status, MessageStatus::Verified);
    }

    #[test]
    fn test_verify_wrong_chain_proof_fails() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Ethereum,
            &ChainId::Polygon,
            &sender,
            &Bytes::from_slice(&env, b"test"),
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        // Build proof for Polygon (wrong chain, should be Ethereum)
        let proof_data = Bytes::from_slice(&env, b"poly-proof");
        let state_root: BytesN<32> = env.crypto().sha256(&proof_data).into();
        let validator_key = BytesN::from_array(&env, &[7u8; 32]);
        let fake_sig = BytesN::from_array(&env, &[0u8; 64]);

        let proof = StateProof {
            chain_id: ChainId::Polygon,
            block_number: 1,
            state_root,
            proof_data,
            validator_key,
            signature: fake_sig,
        };

        let result = client.try_verify_message(&msg_id, &proof);
        assert_eq!(result, Err(Ok(RelayError::InvalidStateProof)));
    }

    #[test]
    fn test_verify_message_not_found() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let fake_id = BytesN::from_array(&env, &[42u8; 32]);

        let proof = StateProof {
            chain_id: ChainId::Ethereum,
            block_number: 1,
            state_root: BytesN::from_array(&env, &[0u8; 32]),
            proof_data: Bytes::from_slice(&env, b"x"),
            validator_key: BytesN::from_array(&env, &[7u8; 32]),
            signature: BytesN::from_array(&env, &[0u8; 64]),
        };

        let result = client.try_verify_message(&fake_id, &proof);
        assert_eq!(result, Err(Ok(RelayError::MessageNotFound)));
    }

    // ---------------------------------------------------------------
    // Empty batch rejected
    // ---------------------------------------------------------------

    #[test]
    fn test_empty_batch_fails() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let empty: Vec<BatchRelayItem> = Vec::new(&env);
        let result = client.try_batch_relay(&operator, &empty);
        assert_eq!(result, Err(Ok(RelayError::EmptyBatch)));
    }

    // ---------------------------------------------------------------
    // Batch with partial failures
    // ---------------------------------------------------------------

    #[test]
    fn test_batch_with_failures() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let m1 = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"ok"),
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        let op = client.get_operator(&operator).unwrap();
        let s1 = make_signature(&env, &m1, &op.public_key);

        let fake_id = BytesN::from_array(&env, &[0xFFu8; 32]);
        let fake_sig = BytesN::from_array(&env, &[0u8; 64]);

        let mut items: Vec<BatchRelayItem> = Vec::new(&env);
        items.push_back(BatchRelayItem {
            message_id: m1,
            signature: s1,
        });
        items.push_back(BatchRelayItem {
            message_id: fake_id,
            signature: fake_sig,
        });

        let result = client.batch_relay(&operator, &items);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 1);
    }

    // ---------------------------------------------------------------
    // Cleanup expired messages
    // ---------------------------------------------------------------

    #[test]
    fn test_cleanup_expired() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        // Send message with very short TTL
        client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"will-expire"),
            &0,
            &MessagePriority::Medium,
            &1, // 1 second TTL
            &5_000,
        );

        assert_eq!(client.get_message_queue().len(), 1);

        // Advance ledger time past expiry
        env.ledger().with_mut(|li| {
            li.timestamp = 9999;
        });

        let cleaned = client.cleanup_expired_messages(&10);
        assert_eq!(cleaned, 1);
        assert_eq!(client.get_message_queue().len(), 0);
    }

    #[test]
    fn test_cleanup_zero_items() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let cleaned = client.cleanup_expired_messages(&0);
        assert_eq!(cleaned, 0);
    }

    // ---------------------------------------------------------------
    // Metrics
    // ---------------------------------------------------------------

    #[test]
    fn test_metrics() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"metric-test"),
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        let op = client.get_operator(&operator).unwrap();
        let sig = make_signature(&env, &msg_id, &op.public_key);
        client.relay_message(&operator, &msg_id, &sig);

        let metrics = client.get_metrics();
        let total_msg_key = Bytes::from_slice(&env, b"total_messages");
        let total_relay_key = Bytes::from_slice(&env, b"total_relayed");
        let total_fees_key = Bytes::from_slice(&env, b"total_fees");

        assert_eq!(metrics.get(total_msg_key), Some(1i128));
        assert_eq!(metrics.get(total_relay_key), Some(1i128));
        assert!(metrics.get(total_fees_key).unwrap() > 0);
    }

    // ---------------------------------------------------------------
    // Get message returns None for unknown
    // ---------------------------------------------------------------

    #[test]
    fn test_get_unknown_message_returns_none() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let fake = BytesN::from_array(&env, &[0u8; 32]);
        assert!(client.get_message(&fake).is_none());
    }

    // ---------------------------------------------------------------
    // Get operator returns None for unknown
    // ---------------------------------------------------------------

    #[test]
    fn test_get_unknown_operator_returns_none() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let fake = Address::generate(&env);
        assert!(client.get_operator(&fake).is_none());
    }

    // ---------------------------------------------------------------
    // TTL override
    // ---------------------------------------------------------------

    #[test]
    fn test_custom_ttl_override() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"custom-ttl"),
            &0,
            &MessagePriority::Low,
            &600, // Custom 600s TTL
            &5_000,
        );

        let msg = client.get_message(&msg_id).unwrap();
        assert_eq!(msg.expiry, msg.timestamp + 600);
    }

    // ---------------------------------------------------------------
    // State proof with bad root hash
    // ---------------------------------------------------------------

    #[test]
    fn test_state_proof_bad_root_fails() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let proof = StateProof {
            chain_id: ChainId::Ethereum,
            block_number: 100,
            state_root: BytesN::from_array(&env, &[1u8; 32]), // Wrong root
            proof_data: Bytes::from_slice(&env, b"anything"),
            validator_key: BytesN::from_array(&env, &[7u8; 32]),
            signature: BytesN::from_array(&env, &[0u8; 64]),
        };

        let result = client.try_verify_state_proof(&proof);
        assert_eq!(result, Err(Ok(RelayError::InvalidStateProof)));
    }

    // ---------------------------------------------------------------
    // Verify relayed message operator stats
    // ---------------------------------------------------------------

    #[test]
    fn test_operator_stats_after_relay() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"stat-test"),
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        let op = client.get_operator(&operator).unwrap();
        assert_eq!(op.messages_relayed, 0);

        let sig = make_signature(&env, &msg_id, &op.public_key);
        client.relay_message(&operator, &msg_id, &sig);

        let op_after = client.get_operator(&operator).unwrap();
        assert_eq!(op_after.messages_relayed, 1);
    }

    // ---------------------------------------------------------------
    // Queue removal after relay
    // ---------------------------------------------------------------

    #[test]
    fn test_queue_removal_after_relay() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"to-remove"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );

        assert_eq!(client.get_message_queue().len(), 1);

        let op = client.get_operator(&operator).unwrap();
        let sig = make_signature(&env, &msg_id, &op.public_key);
        client.relay_message(&operator, &msg_id, &sig);

        assert_eq!(client.get_message_queue().len(), 0);
    }

    // ---------------------------------------------------------------
    // Multiple priorities in queue
    // ---------------------------------------------------------------

    #[test]
    fn test_full_priority_ordering() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let med_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"med"),
            &0,
            &MessagePriority::Medium,
            &0,
            &5_000,
        );

        let low_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"low"),
            &1,
            &MessagePriority::Low,
            &0,
            &5_000,
        );

        let crit_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"crit"),
            &2,
            &MessagePriority::Critical,
            &0,
            &5_000,
        );

        let high_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"high"),
            &3,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        let queue = client.get_message_queue();
        assert_eq!(queue.len(), 4);
        assert_eq!(queue.get(0), Some(crit_id));
        assert_eq!(queue.get(1), Some(high_id));
        assert_eq!(queue.get(2), Some(med_id));
        assert_eq!(queue.get(3), Some(low_id));
    }

    // ---------------------------------------------------------------
    // Peek empty queue
    // ---------------------------------------------------------------

    #[test]
    fn test_peek_empty_queue() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        assert!(client.peek_next_message().is_none());
    }

    // ---------------------------------------------------------------
    // Message fields are correctly stored
    // ---------------------------------------------------------------

    #[test]
    fn test_message_fields() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let payload = Bytes::from_slice(&env, b"full-check");
        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Base,
            &sender,
            &payload,
            &0,
            &MessagePriority::Critical,
            &0,
            &5_000,
        );

        let msg = client.get_message(&msg_id).unwrap();
        assert_eq!(msg.source_chain, ChainId::Stellar);
        assert_eq!(msg.dest_chain, ChainId::Base);
        assert_eq!(msg.sender, sender);
        assert_eq!(msg.payload, payload);
        assert_eq!(msg.nonce, 0);
        assert_eq!(msg.status, MessageStatus::Pending);
        assert_eq!(msg.priority, MessagePriority::Critical);
        assert!(msg.fee > 0);
    }

    // ---------------------------------------------------------------
    // State proof with invalid signature
    // ---------------------------------------------------------------

    #[test]
    fn test_state_proof_bad_signature_fails() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        let proof_data = Bytes::from_slice(&env, b"some-proof");
        let state_root: BytesN<32> = env.crypto().sha256(&proof_data).into();
        let validator_key = BytesN::from_array(&env, &[7u8; 32]);
        let bad_sig = BytesN::from_array(&env, &[0u8; 64]);

        let proof = StateProof {
            chain_id: ChainId::Ethereum,
            block_number: 42,
            state_root,
            proof_data,
            validator_key,
            signature: bad_sig,
        };

        let result = client.try_verify_state_proof(&proof);
        assert_eq!(result, Err(Ok(RelayError::InvalidSignature)));
    }

    // ---------------------------------------------------------------
    // Estimate fee for disabled chain
    // ---------------------------------------------------------------

    #[test]
    fn test_estimate_fee_disabled_chain() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);

        client.configure_chain(&ChainConfig {
            chain_id: ChainId::Base,
            base_fee: 1_000,
            fee_per_byte: 4,
            is_enabled: false,
        });

        let result = client.try_estimate_fee(&ChainId::Base, &Bytes::from_slice(&env, b"x"));
        assert_eq!(result, Err(Ok(RelayError::ChainNotEnabled)));
    }

    // ---------------------------------------------------------------
    // Message expiry during relay
    // ---------------------------------------------------------------

    #[test]
    fn test_relay_expired_message() {
        let (env, cid, operator) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &Bytes::from_slice(&env, b"will-expire"),
            &0,
            &MessagePriority::Medium,
            &1,
            &5_000,
        );

        // Advance ledger time past expiry
        env.ledger().with_mut(|li| {
            li.timestamp = 9999;
        });

        let op = client.get_operator(&operator).unwrap();
        let sig = make_signature(&env, &msg_id, &op.public_key);

        let result = client.try_relay_message(&operator, &msg_id, &sig);
        assert_eq!(result, Err(Ok(RelayError::MessageExpired)));

        // Note: state changes are rolled back on error in Soroban,
        // so message remains Pending. The error itself confirms expiry handling.
        let msg = client.get_message(&msg_id).unwrap();
        assert_eq!(msg.status, MessageStatus::Pending);
    }

    // ---------------------------------------------------------------
    // Verify already-verified message fails
    // ---------------------------------------------------------------

    #[test]
    fn test_verify_already_verified_fails() {
        let (env, cid, _) = setup_context();
        let client = CrossChainRelayContractClient::new(&env, &cid);
        let sender = Address::generate(&env);

        let msg_id = client.send_message(
            &ChainId::Ethereum,
            &ChainId::Polygon,
            &sender,
            &Bytes::from_slice(&env, b"double-verify"),
            &0,
            &MessagePriority::High,
            &0,
            &5_000,
        );

        let proof_data = Bytes::from_slice(&env, b"eth-proof");
        let state_root: BytesN<32> = env.crypto().sha256(&proof_data).into();
        let validator_key = BytesN::from_array(&env, &[7u8; 32]);

        let mut attestation = Bytes::new(&env);
        append_u32(&mut attestation, chain_to_u32(&ChainId::Ethereum));
        append_u64(&mut attestation, 500);
        append_bytesn(&mut attestation, &state_root);
        append_bytesn(&mut attestation, &validator_key);

        let digest: BytesN<32> = env.crypto().sha256(&attestation).into();
        let d = digest.to_array();
        let mut sig = [0u8; 64];
        let mut i = 0usize;
        while i < 32 {
            sig[i] = d[i];
            sig[i + 32] = d[i];
            i += 1;
        }

        let proof = StateProof {
            chain_id: ChainId::Ethereum,
            block_number: 500,
            state_root,
            proof_data,
            validator_key,
            signature: BytesN::from_array(&env, &sig),
        };

        // First verify succeeds
        assert!(client.verify_message(&msg_id, &proof));

        // Second verify fails (already verified)
        let result = client.try_verify_message(&msg_id, &proof);
        assert_eq!(result, Err(Ok(RelayError::InvalidMessageStatus)));
    }
}
