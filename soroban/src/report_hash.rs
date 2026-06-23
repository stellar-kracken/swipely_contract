use soroban_sdk::{contracttype, Bytes, BytesN, Env, String};

pub const HASH_SCHEMA_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportPayload {
    pub report_type: String,
    pub asset_code: String,
    pub value: i128,
    pub timestamp: u64,
    pub nonce: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportHashResult {
    pub hash: BytesN<32>,
    pub schema_version: u32,
    pub payload: ReportPayload,
}

fn str_to_bytes(env: &Env, s: &String) -> Bytes {
    let len = s.len() as usize;
    let mut buf = [0u8; 256];
    let safe_len = len.min(256);
    s.copy_into_slice(&mut buf[..safe_len]);
    let mut result = Bytes::new(env);
    let mut i = 0;
    while i < safe_len {
        result.push_back(buf[i]);
        i += 1;
    }
    result
}

fn serialize_report_payload(env: &Env, payload: &ReportPayload) -> Bytes {
    let mut data = Bytes::new(env);

    data.extend_from_array(&payload.report_type.len().to_be_bytes());
    let type_bytes = str_to_bytes(env, &payload.report_type);
    data.append(&type_bytes);

    data.extend_from_array(&payload.asset_code.len().to_be_bytes());
    let code_bytes = str_to_bytes(env, &payload.asset_code);
    data.append(&code_bytes);

    data.extend_from_array(&payload.value.to_be_bytes());
    data.extend_from_array(&payload.timestamp.to_be_bytes());
    data.extend_from_array(&payload.nonce.to_be_bytes());

    data
}

pub fn compute_report_hash(env: &Env, payload: &ReportPayload) -> ReportHashResult {
    let serialized = serialize_report_payload(env, payload);
    let hash: BytesN<32> = env.crypto().sha256(&serialized).into();

    ReportHashResult {
        hash,
        schema_version: HASH_SCHEMA_VERSION,
        payload: payload.clone(),
    }
}

pub fn verify_report_hash(env: &Env, payload: &ReportPayload, expected_hash: &BytesN<32>) -> bool {
    let result = compute_report_hash(env, payload);
    result.hash == *expected_hash
}

pub fn compute_health_report_hash(
    env: &Env,
    asset_code: &String,
    health_score: u32,
    liquidity_score: u32,
    price_stability_score: u32,
    bridge_uptime_score: u32,
    timestamp: u64,
    nonce: u64,
) -> ReportHashResult {
    let payload = ReportPayload {
        report_type: String::from_str(env, "health"),
        asset_code: asset_code.clone(),
        value: i128::from(health_score),
        timestamp,
        nonce,
    };

    let mut hash_bytes = Bytes::new(env);
    hash_bytes.extend_from_array(&asset_code.len().to_be_bytes());
    let code_bytes = str_to_bytes(env, asset_code);
    hash_bytes.append(&code_bytes);

    hash_bytes.extend_from_array(&health_score.to_be_bytes());
    hash_bytes.extend_from_array(&liquidity_score.to_be_bytes());
    hash_bytes.extend_from_array(&price_stability_score.to_be_bytes());
    hash_bytes.extend_from_array(&bridge_uptime_score.to_be_bytes());
    hash_bytes.extend_from_array(&timestamp.to_be_bytes());
    hash_bytes.extend_from_array(&nonce.to_be_bytes());

    let hash: BytesN<32> = env.crypto().sha256(&hash_bytes).into();

    ReportHashResult {
        hash,
        schema_version: HASH_SCHEMA_VERSION,
        payload,
    }
}

pub fn compute_price_report_hash(
    env: &Env,
    asset_code: &String,
    price: i128,
    source: &String,
    timestamp: u64,
    nonce: u64,
) -> ReportHashResult {
    let payload = ReportPayload {
        report_type: String::from_str(env, "price"),
        asset_code: asset_code.clone(),
        value: price,
        timestamp,
        nonce,
    };

    let mut hash_bytes = Bytes::new(env);
    hash_bytes.extend_from_array(&asset_code.len().to_be_bytes());
    let code_bytes = str_to_bytes(env, asset_code);
    hash_bytes.append(&code_bytes);

    hash_bytes.extend_from_array(&price.to_be_bytes());

    hash_bytes.extend_from_array(&source.len().to_be_bytes());
    let source_bytes = str_to_bytes(env, source);
    hash_bytes.append(&source_bytes);

    hash_bytes.extend_from_array(&timestamp.to_be_bytes());
    hash_bytes.extend_from_array(&nonce.to_be_bytes());

    let hash: BytesN<32> = env.crypto().sha256(&hash_bytes).into();

    ReportHashResult {
        hash,
        schema_version: HASH_SCHEMA_VERSION,
        payload,
    }
}

pub fn compute_mismatch_report_hash(
    env: &Env,
    bridge_id: &String,
    asset_code: &String,
    stellar_supply: i128,
    source_chain_supply: i128,
    timestamp: u64,
    nonce: u64,
) -> ReportHashResult {
    let payload = ReportPayload {
        report_type: String::from_str(env, "mismatch"),
        asset_code: asset_code.clone(),
        value: stellar_supply - source_chain_supply,
        timestamp,
        nonce,
    };

    let mut hash_bytes = Bytes::new(env);
    hash_bytes.extend_from_array(&bridge_id.len().to_be_bytes());
    let bridge_bytes = str_to_bytes(env, bridge_id);
    hash_bytes.append(&bridge_bytes);

    hash_bytes.extend_from_array(&asset_code.len().to_be_bytes());
    let code_bytes = str_to_bytes(env, asset_code);
    hash_bytes.append(&code_bytes);

    hash_bytes.extend_from_array(&stellar_supply.to_be_bytes());
    hash_bytes.extend_from_array(&source_chain_supply.to_be_bytes());
    hash_bytes.extend_from_array(&timestamp.to_be_bytes());
    hash_bytes.extend_from_array(&nonce.to_be_bytes());

    let hash: BytesN<32> = env.crypto().sha256(&hash_bytes).into();

    ReportHashResult {
        hash,
        schema_version: HASH_SCHEMA_VERSION,
        payload,
    }
}

pub fn compute_liquidity_report_hash(
    env: &Env,
    asset_pair: &String,
    total_liquidity: i128,
    depth_0_1_pct: i128,
    depth_0_5_pct: i128,
    depth_1_pct: i128,
    depth_5_pct: i128,
    timestamp: u64,
    nonce: u64,
) -> ReportHashResult {
    let payload = ReportPayload {
        report_type: String::from_str(env, "liquidity"),
        asset_code: asset_pair.clone(),
        value: total_liquidity,
        timestamp,
        nonce,
    };

    let mut hash_bytes = Bytes::new(env);
    hash_bytes.extend_from_array(&asset_pair.len().to_be_bytes());
    let pair_bytes = str_to_bytes(env, asset_pair);
    hash_bytes.append(&pair_bytes);

    hash_bytes.extend_from_array(&total_liquidity.to_be_bytes());
    hash_bytes.extend_from_array(&depth_0_1_pct.to_be_bytes());
    hash_bytes.extend_from_array(&depth_0_5_pct.to_be_bytes());
    hash_bytes.extend_from_array(&depth_1_pct.to_be_bytes());
    hash_bytes.extend_from_array(&depth_5_pct.to_be_bytes());
    hash_bytes.extend_from_array(&timestamp.to_be_bytes());
    hash_bytes.extend_from_array(&nonce.to_be_bytes());

    let hash: BytesN<32> = env.crypto().sha256(&hash_bytes).into();

    ReportHashResult {
        hash,
        schema_version: HASH_SCHEMA_VERSION,
        payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn setup() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    #[test]
    fn test_compute_report_hash() {
        let env = setup();
        let payload = ReportPayload {
            report_type: String::from_str(&env, "health"),
            asset_code: String::from_str(&env, "USDC"),
            value: 9500,
            timestamp: 1_000_000,
            nonce: 1,
        };

        let result = compute_report_hash(&env, &payload);
        assert_eq!(result.schema_version, HASH_SCHEMA_VERSION);
        assert_eq!(result.payload.report_type, String::from_str(&env, "health"));
    }

    #[test]
    fn test_verify_report_hash() {
        let env = setup();
        let payload = ReportPayload {
            report_type: String::from_str(&env, "health"),
            asset_code: String::from_str(&env, "USDC"),
            value: 9500,
            timestamp: 1_000_000,
            nonce: 1,
        };

        let result = compute_report_hash(&env, &payload);
        assert!(verify_report_hash(&env, &payload, &result.hash));
    }

    #[test]
    fn test_hash_is_deterministic() {
        let env = setup();
        let payload = ReportPayload {
            report_type: String::from_str(&env, "health"),
            asset_code: String::from_str(&env, "USDC"),
            value: 9500,
            timestamp: 1_000_000,
            nonce: 1,
        };

        let result1 = compute_report_hash(&env, &payload);
        let result2 = compute_report_hash(&env, &payload);
        assert_eq!(result1.hash, result2.hash);
    }

    #[test]
    fn test_different_payloads_different_hashes() {
        let env = setup();
        let payload1 = ReportPayload {
            report_type: String::from_str(&env, "health"),
            asset_code: String::from_str(&env, "USDC"),
            value: 9500,
            timestamp: 1_000_000,
            nonce: 1,
        };
        let payload2 = ReportPayload {
            report_type: String::from_str(&env, "price"),
            asset_code: String::from_str(&env, "USDC"),
            value: 1_000_000,
            timestamp: 1_000_001,
            nonce: 2,
        };

        let result1 = compute_report_hash(&env, &payload1);
        let result2 = compute_report_hash(&env, &payload2);
        assert_ne!(result1.hash, result2.hash);
    }

    #[test]
    fn test_health_report_hash() {
        let env = setup();
        let result = compute_health_report_hash(
            &env,
            &String::from_str(&env, "USDC"),
            95,
            90,
            92,
            88,
            1_000_000,
            1,
        );
        assert_eq!(result.payload.report_type, String::from_str(&env, "health"));
    }

    #[test]
    fn test_price_report_hash() {
        let env = setup();
        let result = compute_price_report_hash(
            &env,
            &String::from_str(&env, "USDC"),
            1_000_000,
            &String::from_str(&env, "oracle1"),
            1_000_000,
            1,
        );
        assert_eq!(result.payload.report_type, String::from_str(&env, "price"));
    }

    #[test]
    fn test_mismatch_report_hash() {
        let env = setup();
        let result = compute_mismatch_report_hash(
            &env,
            &String::from_str(&env, "bridge-1"),
            &String::from_str(&env, "USDC"),
            1_000_000,
            999_000,
            1_000_000,
            1,
        );
        assert_eq!(
            result.payload.report_type,
            String::from_str(&env, "mismatch")
        );
    }

    #[test]
    fn test_liquidity_report_hash() {
        let env = setup();
        let result = compute_liquidity_report_hash(
            &env,
            &String::from_str(&env, "USDC/XLM"),
            10_000_000,
            1_000_000,
            5_000_000,
            8_000_000,
            10_000_000,
            1_000_000,
            1,
        );
        assert_eq!(
            result.payload.report_type,
            String::from_str(&env, "liquidity")
        );
    }

    #[test]
    fn test_verify_wrong_hash_fails() {
        let env = setup();
        let payload = ReportPayload {
            report_type: String::from_str(&env, "health"),
            asset_code: String::from_str(&env, "USDC"),
            value: 9500,
            timestamp: 1_000_000,
            nonce: 1,
        };

        let wrong_hash: BytesN<32> = env.crypto().sha256(&Bytes::new(&env)).into();
        assert!(!verify_report_hash(&env, &payload, &wrong_hash));
    }

    #[test]
    fn test_health_report_hash_deterministic() {
        let env = setup();
        let r1 = compute_health_report_hash(
            &env,
            &String::from_str(&env, "USDC"),
            95,
            90,
            92,
            88,
            1_000_000,
            1,
        );
        let r2 = compute_health_report_hash(
            &env,
            &String::from_str(&env, "USDC"),
            95,
            90,
            92,
            88,
            1_000_000,
            1,
        );
        assert_eq!(r1.hash, r2.hash);
    }
}
