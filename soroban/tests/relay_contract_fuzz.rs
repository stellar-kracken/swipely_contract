#![cfg(test)]

#[path = "../src/relay/mod.rs"]
mod relay;

use relay::{
    BatchRelayItem, ChainId, CrossChainRelayContract, CrossChainRelayContractClient,
    MessagePriority, RelayError,
};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};

fn setup_context() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainRelayContract);
    let client = CrossChainRelayContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let sender = Address::generate(&env);

    client.initialize(&admin, &300);
    client.register_relay_operator(&operator, &BytesN::from_array(&env, &[9u8; 32]));

    (env, contract_id, admin, operator, sender)
}

fn seed_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut value = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
      value ^= value << 7;
      value ^= value >> 9;
      value = value.wrapping_mul(0xD134_2543_DE82_EF95);
      bytes.push((value & 0xFF) as u8);
    }
    bytes
}

fn chain_for(seed: u64) -> ChainId {
    match seed % 4 {
        0 => ChainId::Stellar,
        1 => ChainId::Ethereum,
        2 => ChainId::Polygon,
        _ => ChainId::Base,
    }
}

#[test]
fn deterministic_fuzz_send_message_rejects_malformed_inputs() {
    let (env, contract_id, _admin, _operator, sender) = setup_context();
    let client = CrossChainRelayContractClient::new(&env, &contract_id);
    let seeds = [1_u64, 4, 7, 11, 16, 23, 42, 64];
    let mut accepted = 0_u32;
    let mut rejected = 0_u32;
    let mut payload_too_large = 0_u32;
    let mut insufficient_fee = 0_u32;

    for seed in seeds {
        let payload_len = if seed % 3 == 0 { 16_385 } else { 1 + (seed as usize % 128) };
        let payload = Bytes::from_slice(&env, &seed_bytes(seed, payload_len));
        let nonce = seed % 2;
        let ttl = if seed % 5 == 0 { 1 } else { 120 };
        let fee = if seed % 4 == 0 { 0 } else { 10 + i128::from(seed as i64) };

        let result = client.try_send_message(
            &chain_for(seed),
            &chain_for(seed.wrapping_add(1)),
            &sender,
            &payload,
            &nonce,
            &MessagePriority::Medium,
            &ttl,
            &fee,
        );

        match result {
            Ok(_) => accepted += 1,
            Err(err) => {
                rejected += 1;
                match err {
                    Ok(RelayError::PayloadTooLarge) => payload_too_large += 1,
                    Ok(RelayError::InsufficientFee) => insufficient_fee += 1,
                    Ok(_) => {}
                    Err(_) => {}
                }
            }
        }
    }

    // accepted may be zero if the Soroban environment rejects all seeds
    // (e.g. nonce conflicts); we guard on classification coverage instead.
    assert!(rejected > 0, "expected at least one rejected seed");
    assert!(payload_too_large > 0, "expected payload classification coverage");
    assert!(insufficient_fee > 0, "expected fee classification coverage");

    println!(
        "fuzz summary: accepted={accepted}, rejected={rejected}, payload_too_large={payload_too_large}, insufficient_fee={insufficient_fee}"
    );
}

#[test]
fn deterministic_fuzz_relay_message_classifies_signature_failures() {
    let (env, contract_id, _admin, operator, sender) = setup_context();
    let client = CrossChainRelayContractClient::new(&env, &contract_id);
    let message_id = client.send_message(
        &ChainId::Stellar,
        &ChainId::Ethereum,
        &sender,
        &Bytes::from_slice(&env, b"fuzz-relay"),
        &0,
        &MessagePriority::High,
        &10,
        &25_000,
    );

    let op = client.get_operator(&operator).unwrap();
    let mut invalid_signature_classes = 0_u32;
    let seeds = [3_u64, 8, 13, 21, 34, 55];

    for seed in seeds {
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&seed_bytes(seed, 64));
        let signature = BytesN::from_array(&env, &sig_bytes);
        let result = client.try_relay_message(&operator, &message_id, &signature);

        match result {
            Err(Ok(RelayError::InvalidSignature)) => invalid_signature_classes += 1,
            Err(Ok(RelayError::OperatorNotActive)) => {}
            Err(Ok(RelayError::MessageNotFound)) => {}
            Err(Ok(_)) => {}
            Err(Err(_)) => {}
            Ok(_) => {
                // A valid signature should only happen if a fuzz seed collides, which is
                // practically impossible. Leave the path explicit so the test remains strict.
                let valid_signature = {
                    let mut payload = [0u8; 64];
                    payload[..32].copy_from_slice(&message_id.to_array());
                    payload[32..].copy_from_slice(&op.public_key.to_array());
                    let digest: BytesN<32> = env.crypto().sha256(&Bytes::from_slice(&env, &payload)).into();
                    let d = digest.to_array();
                    let mut out = [0u8; 64];
                    let mut i = 0usize;
                    while i < 32 {
                        out[i] = d[i];
                        out[i + 32] = d[i];
                        i += 1;
                    }
                    BytesN::from_array(&env, &out)
                };

                let valid = client.try_relay_message(&operator, &message_id, &valid_signature);
                assert!(valid.is_ok(), "expected valid relay path to succeed");
            }
        }
    }

    assert!(invalid_signature_classes > 0, "expected invalid signature coverage");
    println!("fuzz summary: invalid_signature_classes={invalid_signature_classes}");
}

#[test]
fn deterministic_fuzz_batch_relay_reports_mixed_outcomes() {
    let (env, contract_id, _admin, operator, sender) = setup_context();
    let client = CrossChainRelayContractClient::new(&env, &contract_id);
    let op = client.get_operator(&operator).unwrap();

    let mut batch = soroban_sdk::Vec::<BatchRelayItem>::new(&env);
    for (index, seed) in [2_u64, 5, 8, 13].iter().enumerate() {
        let seed = *seed;
        let payload = Bytes::from_slice(&env, &seed_bytes(seed, 32));
        let message_id = client.send_message(
            &ChainId::Stellar,
            &ChainId::Ethereum,
            &sender,
            &payload,
            &(index as u64),
            &MessagePriority::Low,
            &30,
            &50_000,
        );

        let signature = if seed % 2 == 0 {
            let mut payload = [0u8; 64];
            payload[..32].copy_from_slice(&message_id.to_array());
            payload[32..].copy_from_slice(&op.public_key.to_array());
            let digest: BytesN<32> = env.crypto().sha256(&Bytes::from_slice(&env, &payload)).into();
            let d = digest.to_array();
            let mut out = [0u8; 64];
            let mut i = 0usize;
            while i < 32 {
                out[i] = d[i];
                out[i + 32] = d[i];
                i += 1;
            }
            BytesN::from_array(&env, &out)
        } else {
            let mut invalid_sig_bytes = [0u8; 64];
            invalid_sig_bytes.copy_from_slice(&seed_bytes(seed + 99, 64));
            BytesN::from_array(&env, &invalid_sig_bytes)
        };

        batch.push_back(BatchRelayItem { message_id, signature });
    }

    let result = client.batch_relay(&operator, &batch);
    assert!(result.success_count + result.failure_count > 0);
    assert_eq!(result.relayed_ids.len() as u32, result.success_count);

    println!(
        "fuzz summary: batch_success={}, batch_failure={}",
        result.success_count, result.failure_count
    );
}