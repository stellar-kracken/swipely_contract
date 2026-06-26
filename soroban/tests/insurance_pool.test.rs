#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

use bridge_watch_soroban::insurance_pool::{
    ClaimStatus, CoverageTier, InsurancePoolContract, InsurancePoolContractClient,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns (env, client, admin, approver, staker, buyer, pool_id).
/// Governance threshold = 2 (admin + approver must both sign off on claims).
fn setup() -> (
    Env,
    InsurancePoolContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    String,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);

    let contract_id = env.register_contract(None, InsurancePoolContract);
    let client = InsurancePoolContractClient::new(&env, &contract_id);

    let admin    = Address::generate(&env);
    let approver = Address::generate(&env);
    let staker   = Address::generate(&env);
    let buyer    = Address::generate(&env);
    let pool_id  = String::from_str(&env, "USDC_POOL");

    client.initialize(&admin);

    let mut approvers = Vec::new(&env);
    approvers.push_back(admin.clone());
    approvers.push_back(approver.clone());
    // threshold = 2, withdrawal_delay = 120 s
    client.configure_governance(&admin, &approvers, &2u32, &120u64);
    client.create_pool(
        &admin,
        &pool_id,
        &String::from_str(&env, "USDC"),
        &500u32,   // premium_rate_bps
        &1_000u32, // risk_score_bps
    );

    (env, client, admin, approver, staker, buyer, pool_id)
}

fn hash(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

// ── Deposit / stake ───────────────────────────────────────────────────────────

#[test]
fn test_stake_liquidity_updates_pool_and_position() {
    let (_env, client, _admin, _approver, staker, _buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &10_000);

    let pool = client.get_pool(&pool_id).unwrap();
    assert_eq!(pool.staked_liquidity, 10_000);
    assert_eq!(pool.total_liquidity, 10_000);

    let pos = client.get_staker_position(&staker, &pool_id).unwrap();
    assert_eq!(pos.staked_amount, 10_000);
}

// ── Coverage purchase ─────────────────────────────────────────────────────────

#[test]
fn test_purchase_coverage_charges_exact_quoted_premium() {
    let (env, client, _admin, _approver, staker, buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &20_000);

    let quoted = client.quote_premium(&pool_id, &5_000, &CoverageTier::Balanced);
    assert!(quoted > 0);

    let charged = client.purchase_coverage(
        &buyer,
        &pool_id,
        &5_000,
        &CoverageTier::Balanced,
        &(quoted + 10), // max_premium slightly above
    );
    assert_eq!(charged, quoted);
}

#[test]
fn test_coverage_tier_affects_premium() {
    let (env, client, _admin, _approver, staker, _buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &30_000);
    let conservative = client.quote_premium(&pool_id, &5_000, &CoverageTier::Conservative);
    let balanced     = client.quote_premium(&pool_id, &5_000, &CoverageTier::Balanced);
    let aggressive   = client.quote_premium(&pool_id, &5_000, &CoverageTier::Aggressive);

    // Aggressive multiplier (1400 bps) > Balanced (1000) > Conservative (800)
    assert!(aggressive > balanced);
    assert!(balanced > conservative);
}

// ── Claim lifecycle: submit → verify → approve (2-of-2) → payout ─────────────

#[test]
fn test_full_claim_lifecycle_approved_and_paid() {
    let (env, client, admin, approver, staker, buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &20_000);
    let quoted = client.quote_premium(&pool_id, &6_000, &CoverageTier::Balanced);
    client.purchase_coverage(&buyer, &pool_id, &6_000, &CoverageTier::Balanced, &(quoted + 10));

    let claim_id = client.submit_claim(
        &buyer,
        &pool_id,
        &3_000,
        &hash(&env, "QmEvidence"),
    );

    // Verify as genuine (0 slash bps)
    client.verify_claim(&admin, &claim_id, &true, &0u32);

    // First approval — threshold not reached yet
    client.approve_claim(&admin, &claim_id);
    assert_eq!(client.get_claim(&claim_id).unwrap().status, ClaimStatus::Verified);

    // Second approval — threshold reached → Approved
    client.approve_claim(&approver, &claim_id);
    assert_eq!(client.get_claim(&claim_id).unwrap().status, ClaimStatus::Approved);

    // Execute payout
    client.execute_payout(&admin, &claim_id);
    let pool = client.get_pool(&pool_id).unwrap();
    assert_eq!(pool.paid_claims, 1);
    assert_eq!(pool.payout_total, 3_000);
}

// ── Fraudulent claim ──────────────────────────────────────────────────────────

#[test]
fn test_fraudulent_claim_is_rejected_with_slashing() {
    let (env, client, admin, _approver, staker, _buyer, pool_id) = setup();
    let buyer = staker.clone(); // staker is also buyer so slash reduces their position

    client.stake_liquidity(&staker, &pool_id, &8_000);
    let quoted = client.quote_premium(&pool_id, &3_000, &CoverageTier::Balanced);
    client.purchase_coverage(&buyer, &pool_id, &3_000, &CoverageTier::Balanced, &quoted);

    let claim_id = client.submit_claim(&buyer, &pool_id, &2_000, &hash(&env, "QmFraud"));

    // Verify as fraudulent with 2500 bps slash
    client.verify_claim(&admin, &claim_id, &false, &2_500u32);

    let claim = client.get_claim(&claim_id).unwrap();
    assert_eq!(claim.status, ClaimStatus::Rejected);
    assert!(claim.slashed_amount > 0);

    let pool = client.get_pool(&pool_id).unwrap();
    assert_eq!(pool.rejected_claims, 1);
}

// ── Withdrawal queue ──────────────────────────────────────────────────────────

#[test]
fn test_withdrawal_succeeds_after_delay() {
    let (env, client, _admin, _approver, staker, _buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &10_000);
    let req_id = client.request_withdrawal(&staker, &pool_id, &3_000);

    // Move past 120-second delay
    env.ledger().with_mut(|li| li.timestamp += 130);
    let withdrawn = client.execute_withdrawal(&staker, &pool_id, &req_id);
    assert_eq!(withdrawn, 3_000);

    let pos = client.get_staker_position(&staker, &pool_id).unwrap();
    assert_eq!(pos.staked_amount, 7_000);
}

#[test]
#[should_panic]
fn test_withdrawal_before_delay_panics() {
    let (env, client, _admin, _approver, staker, _buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &5_000);
    let req_id = client.request_withdrawal(&staker, &pool_id, &1_000);

    // Only 50 s elapsed — before the 120-second unlock
    env.ledger().with_mut(|li| li.timestamp += 50);
    client.execute_withdrawal(&staker, &pool_id, &req_id);
}

// ── Authorization panics ──────────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_non_approver_cannot_approve_claim() {
    let (env, client, admin, _approver, staker, buyer, pool_id) = setup();
    let rogue = Address::generate(&env);

    client.stake_liquidity(&staker, &pool_id, &10_000);
    let quoted = client.quote_premium(&pool_id, &2_000, &CoverageTier::Balanced);
    client.purchase_coverage(&buyer, &pool_id, &2_000, &CoverageTier::Balanced, &quoted);

    let claim_id = client.submit_claim(&buyer, &pool_id, &1_000, &hash(&env, "QmHash"));
    client.verify_claim(&admin, &claim_id, &true, &0u32);
    client.approve_claim(&rogue, &claim_id); // should panic: "not an approver"
}

#[test]
#[should_panic]
fn test_duplicate_approval_panics() {
    let (env, client, admin, _approver, staker, buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &10_000);
    let quoted = client.quote_premium(&pool_id, &1_500, &CoverageTier::Balanced);
    client.purchase_coverage(&buyer, &pool_id, &1_500, &CoverageTier::Balanced, &quoted);

    let claim_id = client.submit_claim(&buyer, &pool_id, &500, &hash(&env, "QmDuplicate"));
    client.verify_claim(&admin, &claim_id, &true, &0u32);
    client.approve_claim(&admin, &claim_id);
    client.approve_claim(&admin, &claim_id); // should panic: "already approved"
}

#[test]
#[should_panic]
fn test_coverage_cap_exceeded_panics() {
    let (env, client, _admin, _approver, staker, buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &10_000);
    // Conservative cap = 50 % of pool = 5 000; requesting 9 000 exceeds it
    let quoted = client.quote_premium(&pool_id, &9_000, &CoverageTier::Conservative);
    client.purchase_coverage(&buyer, &pool_id, &9_000, &CoverageTier::Conservative, &quoted);
}

// ── Risk score ────────────────────────────────────────────────────────────────

#[test]
fn test_higher_risk_score_increases_premium() {
    let (env, client, admin, _approver, staker, _buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &10_000);
    let low_risk = client.quote_premium(&pool_id, &2_000, &CoverageTier::Balanced);

    client.set_risk_score(&admin, &pool_id, &5_000u32);
    let high_risk = client.quote_premium(&pool_id, &2_000, &CoverageTier::Balanced);

    assert!(high_risk > low_risk);
}

// ── Premium accumulation ──────────────────────────────────────────────────────

#[test]
fn test_staker_earns_premium_after_coverage_purchased() {
    let (env, client, admin, approver, staker, buyer, pool_id) = setup();

    client.stake_liquidity(&staker, &pool_id, &20_000);
    let quoted = client.quote_premium(&pool_id, &5_000, &CoverageTier::Balanced);
    client.purchase_coverage(&buyer, &pool_id, &5_000, &CoverageTier::Balanced, &(quoted + 1));

    // Complete a small claim to settle the pool and distribute premiums
    let claim_id = client.submit_claim(&buyer, &pool_id, &500, &hash(&env, "QmPremium"));
    client.verify_claim(&admin, &claim_id, &true, &0u32);
    client.approve_claim(&admin, &claim_id);
    client.approve_claim(&approver, &claim_id);
    client.execute_payout(&admin, &claim_id);

    let earned = client.claim_premium(&staker, &pool_id);
    assert!(earned > 0);
}
