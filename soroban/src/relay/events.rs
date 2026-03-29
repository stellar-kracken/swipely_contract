//! Event helpers for the Cross-Chain Relay contract.
//!
//! All events follow a `(topic, topic, ...)` + `data` pattern so that they
//! are easy to index on Horizon / RPC.

use soroban_sdk::{symbol_short, Address, BytesN, Env};

use super::types::{ChainId, MessageStatus};

/// Emitted when a new message is submitted.
pub fn emit_message_sent(
    env: &Env,
    message_id: &BytesN<32>,
    sender: &Address,
    dest_chain: &ChainId,
) {
    env.events().publish(
        (symbol_short!("msg_sent"), sender.clone()),
        (message_id.clone(), dest_chain.clone()),
    );
}

/// Emitted when a message status changes.
pub fn emit_message_status_changed(
    env: &Env,
    message_id: &BytesN<32>,
    old_status: &MessageStatus,
    new_status: &MessageStatus,
) {
    env.events().publish(
        (symbol_short!("msg_stat"),),
        (message_id.clone(), old_status.clone(), new_status.clone()),
    );
}

/// Emitted when a message is relayed.
pub fn emit_message_relayed(env: &Env, message_id: &BytesN<32>, operator: &Address) {
    env.events().publish(
        (symbol_short!("msg_rly"), operator.clone()),
        message_id.clone(),
    );
}

/// Emitted when a relay operator is registered.
pub fn emit_operator_registered(env: &Env, operator: &Address) {
    env.events()
        .publish((symbol_short!("op_reg"),), operator.clone());
}

/// Emitted when a relay operator is deactivated.
pub fn emit_operator_deactivated(env: &Env, operator: &Address) {
    env.events()
        .publish((symbol_short!("op_deact"),), operator.clone());
}

/// Emitted when a state proof is verified.
pub fn emit_state_proof_verified(env: &Env, chain_id: &ChainId, block_number: u64) {
    env.events().publish(
        (symbol_short!("st_proof"),),
        (chain_id.clone(), block_number),
    );
}

/// Emitted when a batch relay completes.
pub fn emit_batch_relayed(env: &Env, success_count: u32, failure_count: u32) {
    env.events()
        .publish((symbol_short!("batch"),), (success_count, failure_count));
}

/// Emitted when expired messages are cleaned up.
pub fn emit_messages_cleaned(env: &Env, count: u32) {
    env.events().publish((symbol_short!("cleaned"),), count);
}
