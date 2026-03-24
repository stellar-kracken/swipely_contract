//! Custom error codes for the Cross-Chain Relay contract.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RelayError {
    /// The contract has already been initialised.
    AlreadyInitialized = 1,
    /// The contract has not been initialised.
    NotInitialized = 2,
    /// Caller is not the contract administrator.
    Unauthorized = 3,
    /// The nonce supplied does not match the expected value.
    InvalidNonce = 4,
    /// The message has expired.
    MessageExpired = 5,
    /// The message was not found.
    MessageNotFound = 6,
    /// The message is in an invalid state for this operation.
    InvalidMessageStatus = 7,
    /// The relay operator is not registered or is inactive.
    OperatorNotActive = 8,
    /// The relay operator is already registered.
    OperatorAlreadyRegistered = 9,
    /// Signature verification failed.
    InvalidSignature = 10,
    /// State proof verification failed.
    InvalidStateProof = 11,
    /// The target chain is not enabled.
    ChainNotEnabled = 12,
    /// The target chain configuration was not found.
    ChainConfigNotFound = 13,
    /// Insufficient fee attached to the message.
    InsufficientFee = 14,
    /// The message payload exceeds the maximum allowed size.
    PayloadTooLarge = 15,
    /// The batch is empty.
    EmptyBatch = 16,
    /// The TTL value is invalid.
    InvalidTtl = 17,
}
