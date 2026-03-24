//! Data types for the Cross-Chain Message Relay contract.
//!
//! Defines all structures used for cross-chain message passing, relay operator
//! management, state verification, and fee estimation.

use soroban_sdk::{contracttype, Address, Bytes, BytesN, Vec};

// ---------------------------------------------------------------------------
// Chain identification
// ---------------------------------------------------------------------------

/// Supported destination / source chains.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ChainId {
    Stellar = 0,
    Ethereum = 1,
    Polygon = 2,
    Base = 3,
}

// ---------------------------------------------------------------------------
// Message status
// ---------------------------------------------------------------------------

/// Life-cycle status of a cross-chain message.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MessageStatus {
    Pending = 0,
    Verified = 1,
    Relayed = 2,
    Failed = 3,
    Expired = 4,
}

// ---------------------------------------------------------------------------
// Message priority
// ---------------------------------------------------------------------------

/// Priority level for queue ordering (lower number = higher priority).
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MessagePriority {
    Critical = 0,
    High = 1,
    Medium = 2,
    Low = 3,
}

// ---------------------------------------------------------------------------
// Core message
// ---------------------------------------------------------------------------

/// A cross-chain message.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainMessage {
    /// Unique message identifier (SHA-256 of the canonical payload).
    pub message_id: BytesN<32>,
    /// Source chain.
    pub source_chain: ChainId,
    /// Destination chain.
    pub dest_chain: ChainId,
    /// Sender address (on Stellar this is a Soroban `Address`).
    pub sender: Address,
    /// Arbitrary payload.
    pub payload: Bytes,
    /// Monotonically increasing per-sender nonce (replay protection).
    pub nonce: u64,
    /// Ledger timestamp when the message was submitted.
    pub timestamp: u64,
    /// Ledger timestamp after which the message is considered expired.
    pub expiry: u64,
    /// Priority level.
    pub priority: MessagePriority,
    /// Current status.
    pub status: MessageStatus,
    /// Estimated fee (in stroops).
    pub fee: i128,
}

// ---------------------------------------------------------------------------
// State proof
// ---------------------------------------------------------------------------

/// A proof attesting to the state of a source chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateProof {
    /// The chain whose state is being proved.
    pub chain_id: ChainId,
    /// Block / ledger number at which the proof was generated.
    pub block_number: u64,
    /// State root hash of the source chain block.
    pub state_root: BytesN<32>,
    /// Serialised Merkle / MPT proof blob.
    pub proof_data: Bytes,
    /// Ed25519 public key of the validator that signed the proof.
    pub validator_key: BytesN<32>,
    /// Ed25519 signature over `sha256(chain_id || block_number || state_root)`.
    pub signature: BytesN<64>,
}

// ---------------------------------------------------------------------------
// Relay operator
// ---------------------------------------------------------------------------

/// A whitelisted relay operator.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayOperator {
    /// Soroban `Address` of the operator.
    pub operator: Address,
    /// Ed25519 public key used to verify operator signatures.
    pub public_key: BytesN<32>,
    /// Whether the operator is currently active.
    pub is_active: bool,
    /// Number of messages successfully relayed.
    pub messages_relayed: u64,
    /// Timestamp of registration.
    pub registered_at: u64,
}

// ---------------------------------------------------------------------------
// Gas / fee estimation
// ---------------------------------------------------------------------------

/// Per-chain gas / fee configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChainConfig {
    /// Chain identifier.
    pub chain_id: ChainId,
    /// Base fee for a message (stroops).
    pub base_fee: i128,
    /// Additional fee per byte of payload (stroops).
    pub fee_per_byte: i128,
    /// Whether relaying to this chain is currently enabled.
    pub is_enabled: bool,
}

// ---------------------------------------------------------------------------
// Batch relay item
// ---------------------------------------------------------------------------

/// A single item within a batched relay request.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRelayItem {
    /// The message identifier.
    pub message_id: BytesN<32>,
    /// Ed25519 signature of the relay operator over the message id.
    pub signature: BytesN<64>,
}

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Storage key enumeration for the relay contract.
#[contracttype]
#[derive(Clone)]
pub enum RelayDataKey {
    /// Contract administrator.
    Admin,
    /// Whether the contract has been initialised.
    Initialized,
    /// A cross-chain message keyed by its id.
    Message(BytesN<32>),
    /// Ordered list of pending message ids (the queue).
    MessageQueue,
    /// Nonce counter for a sender address.
    Nonce(Address),
    /// A relay operator keyed by address.
    Operator(Address),
    /// List of all operator addresses.
    OperatorList,
    /// Chain configuration keyed by a u32 (ChainId as u32).
    ChainConfig(u32),
    /// Total number of messages sent.
    TotalMessages,
    /// Total number of messages relayed.
    TotalRelayed,
    /// Total fees collected.
    TotalFees,
    /// Default message TTL in seconds.
    DefaultTtl,
}

// ---------------------------------------------------------------------------
// Relay result (returned from relay / batch_relay)
// ---------------------------------------------------------------------------

/// Result of a relay or batch relay operation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayResult {
    /// Number of messages successfully relayed.
    pub success_count: u32,
    /// Number of messages that failed.
    pub failure_count: u32,
    /// Message ids that were successfully relayed.
    pub relayed_ids: Vec<BytesN<32>>,
}
