#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    StakerPosition(Address),
    CoveragePool(String),
    InsuranceClaim(u64),
    ClaimCount,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolInfo {
    pub pool_id: String,
    pub total_liquidity: i128,
    pub active_coverage: i128,
    pub premium_rate_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClaimStatus {
    Submitted,
    Verified,
    Approved,
    Rejected,
    Paid,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimInfo {
    pub claim_id: u64,
    pub claimant: Address,
    pub pool_id: String,
    pub amount: i128,
    pub status: ClaimStatus,
}

#[contract]
pub struct InsurancePoolContract;

#[contractimpl]
impl InsurancePoolContract {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ClaimCount, &0u64);
    }

    pub fn stake_liquidity(env: Env, staker: Address, pool_id: String, amount: i128) {
        staker.require_auth();
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        let mut staker_pos = env.storage().instance().get(&DataKey::StakerPosition(staker.clone())).unwrap_or(0i128);
        staker_pos += amount;
        env.storage().instance().set(&DataKey::StakerPosition(staker), &staker_pos);

        let mut pool: PoolInfo = env.storage().instance().get(&DataKey::CoveragePool(pool_id.clone())).unwrap_or(PoolInfo {
            pool_id: pool_id.clone(),
            total_liquidity: 0,
            active_coverage: 0,
            premium_rate_bps: 500,
        });
        pool.total_liquidity += amount;
        env.storage().instance().set(&DataKey::CoveragePool(pool_id), &pool);
    }

    pub fn request_withdrawal(env: Env, staker: Address, pool_id: String, amount: i128) {
        staker.require_auth();
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        let mut staker_pos: i128 = env.storage().instance().get(&DataKey::StakerPosition(staker.clone())).unwrap_or(0);
        if staker_pos < amount {
            panic!("Insufficient staked balance");
        }
        staker_pos -= amount;
        env.storage().instance().set(&DataKey::StakerPosition(staker), &staker_pos);

        let mut pool: PoolInfo = env.storage().instance().get(&DataKey::CoveragePool(pool_id.clone())).unwrap();
        pool.total_liquidity -= amount;
        env.storage().instance().set(&DataKey::CoveragePool(pool_id), &pool);
    }

    pub fn purchase_coverage(env: Env, buyer: Address, pool_id: String, coverage_amount: i128, _premium_paid: i128) {
        buyer.require_auth();
        if coverage_amount <= 0 {
            panic!("Coverage amount must be positive");
        }
        let mut pool: PoolInfo = env.storage().instance().get(&DataKey::CoveragePool(pool_id.clone())).unwrap();
        if pool.total_liquidity - pool.active_coverage < coverage_amount {
            panic!("Insufficient available liquidity");
        }
        pool.active_coverage += coverage_amount;
        env.storage().instance().set(&DataKey::CoveragePool(pool_id), &pool);
    }

    pub fn submit_claim(env: Env, claimant: Address, pool_id: String, amount: i128) -> u64 {
        claimant.require_auth();
        if amount <= 0 {
            panic!("Claim amount must be positive");
        }
        let claim_id: u64 = env.storage().instance().get(&DataKey::ClaimCount).unwrap_or(0);
        let next_id = claim_id + 1;
        env.storage().instance().set(&DataKey::ClaimCount, &next_id);

        let claim = ClaimInfo {
            claim_id,
            claimant,
            pool_id,
            amount,
            status: ClaimStatus::Submitted,
        };
        env.storage().instance().set(&DataKey::InsuranceClaim(claim_id), &claim);
        claim_id
    }

    pub fn verify_claim(env: Env, verifier: Address, claim_id: u64) {
        verifier.require_auth();
        let expected_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if verifier != expected_admin {
            panic!("Not authorized");
        }
        
        let mut claim: ClaimInfo = env.storage().instance().get(&DataKey::InsuranceClaim(claim_id)).unwrap();
        if claim.status != ClaimStatus::Submitted {
            panic!("Invalid status");
        }
        claim.status = ClaimStatus::Verified;
        env.storage().instance().set(&DataKey::InsuranceClaim(claim_id), &claim);
    }

    pub fn approve_claim(env: Env, admin: Address, claim_id: u64) {
        admin.require_auth();
        let expected_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if admin != expected_admin {
            panic!("Not admin");
        }

        let mut claim: ClaimInfo = env.storage().instance().get(&DataKey::InsuranceClaim(claim_id)).unwrap();
        if claim.status != ClaimStatus::Verified {
            panic!("Must be verified before approval"); // requirement step
        }
        claim.status = ClaimStatus::Approved;
        env.storage().instance().set(&DataKey::InsuranceClaim(claim_id), &claim);
    }

    pub fn execute_payout(env: Env, _executor: Address, claim_id: u64) {
        let mut claim: ClaimInfo = env.storage().instance().get(&DataKey::InsuranceClaim(claim_id)).unwrap();
        if claim.status != ClaimStatus::Approved {
            panic!("Claim not approved");
        }
        
        let mut pool: PoolInfo = env.storage().instance().get(&DataKey::CoveragePool(claim.pool_id.clone())).unwrap();
        if pool.total_liquidity < claim.amount {
            panic!("Insufficient pool liquidity");
        }
        
        pool.total_liquidity -= claim.amount;
        pool.active_coverage -= claim.amount;
        env.storage().instance().set(&DataKey::CoveragePool(claim.pool_id.clone()), &pool);

        claim.status = ClaimStatus::Paid;
        env.storage().instance().set(&DataKey::InsuranceClaim(claim_id), &claim);
    }
}

// Tests
#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_insurance_pool_flow() {
        let env = Env::default();
        let contract_id = env.register_contract(None, InsurancePoolContract);
        let client = InsurancePoolContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        
        env.mock_all_auths();
        
        client.initialize(&admin);
        
        let pool_id = String::from_str(&env, "ETH_BRIDGE");
        
        // Stake liquidity
        client.stake_liquidity(&user1, &pool_id, &10000);
        
        // Purchase coverage
        client.purchase_coverage(&user2, &pool_id, &5000, &100);
        
        // Submit claim
        let claim_id = client.submit_claim(&user2, &pool_id, &2000);
        assert_eq!(claim_id, 0);
        
        // Verify claim
        client.verify_claim(&admin, &claim_id);
        
        // Approve claim
        client.approve_claim(&admin, &claim_id);
        
        // Execute payout
        client.execute_payout(&user2, &claim_id);
        
        // Check pool liquidity
        let _pool_info = client.stake_liquidity(&user1, &pool_id, &0);
        // Withdraw remaining
        client.request_withdrawal(&user1, &pool_id, &8000);
    }
    
    #[test]
    #[should_panic(expected = "Insufficient available liquidity")]
    fn test_insufficient_liquidity() {
        let env = Env::default();
        let contract_id = env.register_contract(None, InsurancePoolContract);
        let client = InsurancePoolContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let staker = Address::generate(&env);
        let buyer = Address::generate(&env);
        
        env.mock_all_auths();
        client.initialize(&admin);
        let pool_id = String::from_str(&env, "BTC_BRIDGE");
        
        client.stake_liquidity(&staker, &pool_id, &1000);
        client.purchase_coverage(&buyer, &pool_id, &2000, &50); // should panic
    }
}
