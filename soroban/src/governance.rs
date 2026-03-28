use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String};

// ── Enums ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalType {
    ParameterChange,
    OperatorApproval,
    EmergencyPause,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Active,
    Passed,
    Failed,
    Queued,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoteChoice {
    For,
    Against,
    Abstain,
}

// ── Structs ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u32,
    pub proposer: Address,
    pub proposal_type: ProposalType,
    pub title: String,
    pub description: String,
    pub target_contract: Address,
    pub calldata: String,
    pub deposit: i128,
    pub status: ProposalStatus,
    pub votes_for: i128,
    pub votes_against: i128,
    pub votes_abstain: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub execute_after: u64,
    pub created_at: u64,
    pub executed_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct VoteRecord {
    pub voter: Address,
    pub proposal_id: u32,
    pub choice: VoteChoice,
    pub voting_power: i128,
    pub effective_votes: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct GovernanceConfig {
    pub timelock_delay: u64,
    pub voting_period: u64,
    pub voting_delay: u64,
    pub quorum_bps: u32,
    pub pass_threshold_bps: u32,
    pub proposal_deposit: i128,
    pub use_quadratic: bool,
    pub guardian_threshold: u32,
}

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    Config,
    ProposalCount,
    TotalSupply,
    GuardianCount,
    Proposal(u32),
    VoteRecord(u32, Address),
    Delegation(Address),
    DelegatedPower(Address),
    VotingPower(Address),
    Guardian(Address),
    GuardianApproval(u32, Address),
    GuardianApprovalCount(u32),
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    // ── Init ─────────────────────────────────────────────────────────────────

    pub fn initialize(
        env: Env,
        admin: Address,
        timelock_delay: u64,
        voting_period: u64,
        voting_delay: u64,
        quorum_bps: u32,
        pass_threshold_bps: u32,
        proposal_deposit: i128,
        use_quadratic: bool,
        guardian_threshold: u32,
    ) {
        admin.require_auth();
        assert!(
            !env.storage().instance().has(&DataKey::Admin),
            "already initialised"
        );
        assert!(quorum_bps <= 10_000, "quorum > 100%");
        assert!(
            pass_threshold_bps <= 10_000 && pass_threshold_bps > 0,
            "invalid threshold"
        );

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ProposalCount, &0u32);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
        env.storage().instance().set(&DataKey::GuardianCount, &0u32);

        let cfg = GovernanceConfig {
            timelock_delay,
            voting_period,
            voting_delay,
            quorum_bps,
            pass_threshold_bps,
            proposal_deposit,
            use_quadratic,
            guardian_threshold,
        };
        env.storage().instance().set(&DataKey::Config, &cfg);
    }

    // ── Voting-power management ───────────────────────────────────────────────

    /// Admin registers the voting-power snapshot for a voter (represents token balance).
    pub fn set_voting_power(env: Env, voter: Address, power: i128) {
        Self::only_admin(&env);
        assert!(power >= 0, "power < 0");

        let old: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::VotingPower(voter.clone()))
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&DataKey::VotingPower(voter), &power);

        let mut total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        total = total - old + power;
        env.storage().instance().set(&DataKey::TotalSupply, &total);
    }

    // ── Guardian management ───────────────────────────────────────────────────

    pub fn add_guardian(env: Env, guardian: Address) {
        Self::only_admin(&env);
        let already: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Guardian(guardian.clone()))
            .unwrap_or(false);
        if !already {
            env.storage()
                .persistent()
                .set(&DataKey::Guardian(guardian), &true);
            let mut count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::GuardianCount)
                .unwrap_or(0);
            count += 1;
            env.storage()
                .instance()
                .set(&DataKey::GuardianCount, &count);
        }
    }

    pub fn remove_guardian(env: Env, guardian: Address) {
        Self::only_admin(&env);
        let is_guardian: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Guardian(guardian.clone()))
            .unwrap_or(false);
        if is_guardian {
            env.storage()
                .persistent()
                .set(&DataKey::Guardian(guardian), &false);
            let mut count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::GuardianCount)
                .unwrap_or(0);
            count = count.saturating_sub(1);
            env.storage()
                .instance()
                .set(&DataKey::GuardianCount, &count);
        }
    }

    // ── Delegation ────────────────────────────────────────────────────────────

    /// Delegate all of caller's voting power to `delegatee`.
    pub fn delegate_votes(env: Env, delegator: Address, delegatee: Address) {
        delegator.require_auth();
        assert!(delegator != delegatee, "cannot self-delegate");

        Self::remove_delegation_internal(&env, delegator.clone());

        let power = Self::raw_power(&env, delegator.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Delegation(delegator), &delegatee.clone());

        let mut received: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::DelegatedPower(delegatee.clone()))
            .unwrap_or(0);
        received += power;
        env.storage()
            .persistent()
            .set(&DataKey::DelegatedPower(delegatee), &received);
    }

    /// Remove caller's delegation and reclaim voting power.
    pub fn undelegate_votes(env: Env, delegator: Address) {
        delegator.require_auth();
        Self::remove_delegation_internal(&env, delegator);
    }

    // ── Proposal lifecycle ────────────────────────────────────────────────────

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        proposal_type: ProposalType,
        title: String,
        description: String,
        target_contract: Address,
        calldata: String,
    ) -> u32 {
        proposer.require_auth();

        let cfg: GovernanceConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let proposer_power = Self::effective_power(&env, proposer.clone());
        assert!(
            proposer_power >= cfg.proposal_deposit,
            "insufficient voting power for deposit"
        );

        let now = env.ledger().timestamp();
        let start_time = now + cfg.voting_delay;
        let end_time = start_time + cfg.voting_period;

        let mut count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap();
        let id = count;
        count += 1;
        env.storage()
            .instance()
            .set(&DataKey::ProposalCount, &count);

        let proposal = Proposal {
            id,
            proposer,
            proposal_type,
            title,
            description,
            target_contract,
            calldata,
            deposit: cfg.proposal_deposit,
            status: ProposalStatus::Pending,
            votes_for: 0,
            votes_against: 0,
            votes_abstain: 0,
            start_time,
            end_time,
            execute_after: 0,
            created_at: now,
            executed_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(id), &proposal);

        id
    }

    /// Transition Pending -> Active once the voting delay has passed. Anyone can call.
    pub fn activate_proposal(env: Env, proposal_id: u32) {
        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(proposal.status == ProposalStatus::Pending, "not pending");
        assert!(
            env.ledger().timestamp() >= proposal.start_time,
            "voting delay not elapsed"
        );
        proposal.status = ProposalStatus::Active;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
    }

    pub fn cast_vote(env: Env, voter: Address, proposal_id: u32, choice: VoteChoice) {
        voter.require_auth();

        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(proposal.status == ProposalStatus::Active, "not active");

        let now = env.ledger().timestamp();
        assert!(
            now >= proposal.start_time && now <= proposal.end_time,
            "outside voting window"
        );
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKey::VoteRecord(proposal_id, voter.clone())),
            "already voted"
        );

        let cfg: GovernanceConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let power = Self::effective_power(&env, voter.clone());
        assert!(power > 0, "no voting power");

        let effective_votes = if cfg.use_quadratic {
            Self::isqrt(power as u128) as i128
        } else {
            power
        };

        let record = VoteRecord {
            voter: voter.clone(),
            proposal_id,
            choice: choice.clone(),
            voting_power: power,
            effective_votes,
            timestamp: now,
        };
        env.storage()
            .persistent()
            .set(&DataKey::VoteRecord(proposal_id, voter), &record);

        match choice {
            VoteChoice::For => proposal.votes_for += effective_votes,
            VoteChoice::Against => proposal.votes_against += effective_votes,
            VoteChoice::Abstain => proposal.votes_abstain += effective_votes,
        }

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
    }

    /// Tally votes and mark Passed or Failed. Anyone can call after end_time.
    pub fn finalize_proposal(env: Env, proposal_id: u32) {
        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(proposal.status == ProposalStatus::Active, "not active");
        assert!(
            env.ledger().timestamp() > proposal.end_time,
            "voting still open"
        );

        let cfg: GovernanceConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let total_supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);

        let total_votes = proposal.votes_for + proposal.votes_against + proposal.votes_abstain;

        let quorum_met =
            total_supply > 0 && (total_votes * 10_000) / total_supply >= cfg.quorum_bps as i128;

        let threshold_met = (proposal.votes_for + proposal.votes_against) > 0
            && (proposal.votes_for * 10_000) / (proposal.votes_for + proposal.votes_against)
                >= cfg.pass_threshold_bps as i128;

        proposal.status = if quorum_met && threshold_met {
            ProposalStatus::Passed
        } else {
            ProposalStatus::Failed
        };

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
    }

    /// Queue a Passed proposal for timelock.
    pub fn queue_proposal(env: Env, proposal_id: u32) {
        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(proposal.status == ProposalStatus::Passed, "not passed");

        let cfg: GovernanceConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        proposal.execute_after = env.ledger().timestamp() + cfg.timelock_delay;
        proposal.status = ProposalStatus::Queued;

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
    }

    /// Execute a Queued proposal after the timelock expires.
    pub fn execute_proposal(env: Env, executor: Address, proposal_id: u32) {
        executor.require_auth();

        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(proposal.status == ProposalStatus::Queued, "not queued");
        assert!(
            env.ledger().timestamp() >= proposal.execute_after,
            "timelock not expired"
        );

        proposal.status = ProposalStatus::Executed;
        proposal.executed_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events()
            .publish((symbol_short!("gov"), symbol_short!("exec")), proposal_id);
    }

    // ── Guardian multisig ─────────────────────────────────────────────────────

    pub fn guardian_approve(env: Env, guardian: Address, proposal_id: u32) {
        guardian.require_auth();
        assert!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::Guardian(guardian.clone()))
                .unwrap_or(false),
            "not a guardian"
        );
        assert!(
            !env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::GuardianApproval(proposal_id, guardian.clone()))
                .unwrap_or(false),
            "already approved"
        );

        env.storage()
            .persistent()
            .set(&DataKey::GuardianApproval(proposal_id, guardian), &true);

        let mut count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::GuardianApprovalCount(proposal_id))
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::GuardianApprovalCount(proposal_id), &count);
    }

    /// Emergency execution — bypasses timelock, requires guardian threshold approvals.
    pub fn guardian_execute(env: Env, executor: Address, proposal_id: u32) {
        executor.require_auth();
        assert!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::Guardian(executor.clone()))
                .unwrap_or(false),
            "not a guardian"
        );

        let cfg: GovernanceConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let approvals: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::GuardianApprovalCount(proposal_id))
            .unwrap_or(0);
        assert!(
            approvals >= cfg.guardian_threshold,
            "insufficient approvals"
        );

        let mut proposal = Self::load_proposal(&env, proposal_id);
        assert!(
            proposal.proposal_type == ProposalType::EmergencyPause,
            "only emergency proposals"
        );
        assert!(
            proposal.status == ProposalStatus::Active
                || proposal.status == ProposalStatus::Passed
                || proposal.status == ProposalStatus::Queued,
            "invalid status"
        );

        proposal.status = ProposalStatus::Executed;
        proposal.executed_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events()
            .publish((symbol_short!("gov"), symbol_short!("gexec")), proposal_id);
    }

    /// Cancel a proposal. Only the proposer or admin may cancel.
    pub fn cancel_proposal(env: Env, caller: Address, proposal_id: u32) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let mut proposal = Self::load_proposal(&env, proposal_id);

        assert!(
            caller == proposal.proposer || caller == admin,
            "unauthorised"
        );
        assert!(
            matches!(
                proposal.status,
                ProposalStatus::Pending
                    | ProposalStatus::Active
                    | ProposalStatus::Passed
                    | ProposalStatus::Queued
            ),
            "cannot cancel in current state"
        );

        proposal.status = ProposalStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
    }

    // ── Config updates ────────────────────────────────────────────────────────

    pub fn update_config(
        env: Env,
        timelock_delay: u64,
        voting_period: u64,
        voting_delay: u64,
        quorum_bps: u32,
        pass_threshold_bps: u32,
        proposal_deposit: i128,
        use_quadratic: bool,
        guardian_threshold: u32,
    ) {
        Self::only_admin(&env);
        assert!(quorum_bps <= 10_000, "quorum > 100%");
        assert!(
            pass_threshold_bps <= 10_000 && pass_threshold_bps > 0,
            "invalid threshold"
        );
        let cfg = GovernanceConfig {
            timelock_delay,
            voting_period,
            voting_delay,
            quorum_bps,
            pass_threshold_bps,
            proposal_deposit,
            use_quadratic,
            guardian_threshold,
        };
        env.storage().instance().set(&DataKey::Config, &cfg);
    }

    // ── Getters ───────────────────────────────────────────────────────────────

    pub fn get_proposal(env: Env, proposal_id: u32) -> Proposal {
        Self::load_proposal(&env, proposal_id)
    }

    pub fn get_vote(env: Env, proposal_id: u32, voter: Address) -> Option<VoteRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::VoteRecord(proposal_id, voter))
    }

    pub fn get_config(env: Env) -> GovernanceConfig {
        env.storage().instance().get(&DataKey::Config).unwrap()
    }

    pub fn get_voting_power(env: Env, voter: Address) -> i128 {
        Self::effective_power(&env, voter)
    }

    pub fn get_delegation(env: Env, delegator: Address) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::Delegation(delegator))
    }

    pub fn proposal_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0)
    }

    pub fn is_guardian(env: Env, addr: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Guardian(addr))
            .unwrap_or(false)
    }

    pub fn get_guardian_approvals(env: Env, proposal_id: u32) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::GuardianApprovalCount(proposal_id))
            .unwrap_or(0)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn only_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
    }

    fn load_proposal(env: &Env, id: u32) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(id))
            .expect("proposal not found")
    }

    fn raw_power(env: &Env, voter: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::VotingPower(voter))
            .unwrap_or(0)
    }

    /// Effective power = own power (if not delegated away) + power delegated to this address.
    fn effective_power(env: &Env, voter: Address) -> i128 {
        let has_delegated = env
            .storage()
            .persistent()
            .has(&DataKey::Delegation(voter.clone()));

        let own = if has_delegated {
            0
        } else {
            Self::raw_power(env, voter.clone())
        };

        let received: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::DelegatedPower(voter))
            .unwrap_or(0);

        own + received
    }

    fn remove_delegation_internal(env: &Env, delegator: Address) {
        let existing: Option<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Delegation(delegator.clone()));

        if let Some(delegatee) = existing {
            let power = Self::raw_power(env, delegator.clone());
            let mut received: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::DelegatedPower(delegatee.clone()))
                .unwrap_or(0);
            received = (received - power).max(0);
            env.storage()
                .persistent()
                .set(&DataKey::DelegatedPower(delegatee), &received);
            env.storage()
                .persistent()
                .remove(&DataKey::Delegation(delegator));
        }
    }

    /// Integer square root (floor) — used for quadratic voting.
    fn isqrt(n: u128) -> u128 {
        if n == 0 {
            return 0;
        }
        let mut x = n;
        let mut y = x.div_ceil(2);
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env, String};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup() -> (Env, Address, soroban_sdk::Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, GovernanceContract);
        let client = GovernanceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin, &100,   // timelock_delay
            &200,   // voting_period
            &10,    // voting_delay
            &1000,  // quorum_bps  (10 %)
            &5100,  // pass_threshold_bps (51 %)
            &100,   // proposal_deposit
            &false, // use_quadratic
            &2,     // guardian_threshold
        );

        (env, admin, contract_id)
    }

    fn mk_str(env: &Env, s: &str) -> String {
        String::from_str(env, s)
    }

    fn create_funded_proposal(
        env: &Env,
        client: &GovernanceContractClient,
        proposer: &Address,
        _admin: &Address,
        ptype: ProposalType,
    ) -> u32 {
        let target = Address::generate(env);
        client.set_voting_power(proposer, &1000);
        client.create_proposal(
            proposer,
            &ptype,
            &mk_str(env, "title"),
            &mk_str(env, "desc"),
            &target,
            &mk_str(env, "calldata"),
        )
    }

    // advance ledger time
    fn advance(env: &Env, secs: u64) {
        env.ledger().with_mut(|li| li.timestamp += secs);
    }

    // ── initialize ────────────────────────────────────────────────────────────

    #[test]
    fn test_initialize_ok() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let cfg = client.get_config();
        assert_eq!(cfg.quorum_bps, 1000);
        assert_eq!(cfg.pass_threshold_bps, 5100);
        assert_eq!(cfg.proposal_deposit, 100);
        assert!(!cfg.use_quadratic);
        assert_eq!(client.proposal_count(), 0);
        assert_eq!(client.total_supply(), 0);
    }

    #[test]
    #[should_panic(expected = "already initialised")]
    fn test_initialize_twice_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        client.initialize(&admin, &100, &200, &10, &1000, &5100, &100, &false, &2);
    }

    #[test]
    #[should_panic(expected = "quorum > 100%")]
    fn test_initialize_invalid_quorum() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, GovernanceContract);
        let client = GovernanceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &100, &200, &10, &10_001, &5100, &100, &false, &2);
    }

    #[test]
    #[should_panic(expected = "invalid threshold")]
    fn test_initialize_zero_threshold() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, GovernanceContract);
        let client = GovernanceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &100, &200, &10, &1000, &0, &100, &false, &2);
    }

    // ── set_voting_power ──────────────────────────────────────────────────────

    #[test]
    fn test_set_voting_power_updates_total_supply() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let voter = Address::generate(&env);

        client.set_voting_power(&voter, &500);
        assert_eq!(client.total_supply(), 500);
        assert_eq!(client.get_voting_power(&voter), 500);

        // update
        client.set_voting_power(&voter, &300);
        assert_eq!(client.total_supply(), 300);
    }

    #[test]
    #[should_panic(expected = "power < 0")]
    fn test_set_negative_power_panics() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let voter = Address::generate(&env);
        client.set_voting_power(&voter, &-1);
    }

    // ── guardian management ───────────────────────────────────────────────────

    #[test]
    fn test_add_remove_guardian() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let g = Address::generate(&env);

        assert!(!client.is_guardian(&g));
        client.add_guardian(&g);
        assert!(client.is_guardian(&g));

        // adding again is idempotent
        client.add_guardian(&g);
        assert!(client.is_guardian(&g));

        client.remove_guardian(&g);
        assert!(!client.is_guardian(&g));

        // removing again is safe
        client.remove_guardian(&g);
        assert!(!client.is_guardian(&g));
    }

    // ── delegation ────────────────────────────────────────────────────────────

    #[test]
    fn test_delegate_transfers_voting_power() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let delegator = Address::generate(&env);
        let delegatee = Address::generate(&env);

        client.set_voting_power(&delegator, &400);
        client.delegate_votes(&delegator, &delegatee);

        // delegator loses power, delegatee gains it
        assert_eq!(client.get_voting_power(&delegator), 0);
        assert_eq!(client.get_voting_power(&delegatee), 400);
        assert_eq!(client.get_delegation(&delegator), Some(delegatee.clone()));
    }

    #[test]
    fn test_undelegate_restores_power() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let delegator = Address::generate(&env);
        let delegatee = Address::generate(&env);

        client.set_voting_power(&delegator, &400);
        client.delegate_votes(&delegator, &delegatee);
        client.undelegate_votes(&delegator);

        assert_eq!(client.get_voting_power(&delegator), 400);
        assert_eq!(client.get_voting_power(&delegatee), 0);
        assert_eq!(client.get_delegation(&delegator), None);
    }

    #[test]
    fn test_re_delegation_clears_old_delegatee() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let delegator = Address::generate(&env);
        let d1 = Address::generate(&env);
        let d2 = Address::generate(&env);

        client.set_voting_power(&delegator, &400);
        client.delegate_votes(&delegator, &d1);
        client.delegate_votes(&delegator, &d2); // re-delegate

        assert_eq!(client.get_voting_power(&d1), 0);
        assert_eq!(client.get_voting_power(&d2), 400);
    }

    #[test]
    #[should_panic(expected = "cannot self-delegate")]
    fn test_self_delegation_panics() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let voter = Address::generate(&env);
        client.delegate_votes(&voter, &voter);
    }

    // ── create_proposal ───────────────────────────────────────────────────────

    #[test]
    fn test_create_proposal_increments_count() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        assert_eq!(id, 0);
        assert_eq!(client.proposal_count(), 1);

        let p = client.get_proposal(&id);
        assert_eq!(p.status, ProposalStatus::Pending);
        assert_eq!(p.deposit, 100);
    }

    #[test]
    #[should_panic(expected = "insufficient voting power for deposit")]
    fn test_create_proposal_insufficient_power() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let target = Address::generate(&env);

        // no power set — defaults to 0
        client.create_proposal(
            &proposer,
            &ProposalType::ParameterChange,
            &mk_str(&env, "t"),
            &mk_str(&env, "d"),
            &target,
            &mk_str(&env, "c"),
        );
    }

    // ── activate_proposal ─────────────────────────────────────────────────────

    #[test]
    fn test_activate_proposal_after_delay() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        advance(&env, 10); // voting_delay = 10
        client.activate_proposal(&id);
        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Active);
    }

    #[test]
    #[should_panic(expected = "voting delay not elapsed")]
    fn test_activate_before_delay_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.activate_proposal(&id); // no time advance
    }

    #[test]
    #[should_panic(expected = "not pending")]
    fn test_activate_already_active_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.activate_proposal(&id); // second call panics
    }

    // ── cast_vote ─────────────────────────────────────────────────────────────

    #[test]
    fn test_cast_vote_for() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &500);
        advance(&env, 10);
        client.activate_proposal(&id);

        client.cast_vote(&voter, &id, &VoteChoice::For);
        let p = client.get_proposal(&id);
        assert_eq!(p.votes_for, 500);

        let record = client.get_vote(&id, &voter).unwrap();
        assert_eq!(record.effective_votes, 500);
        assert_eq!(record.choice, VoteChoice::For);
    }

    #[test]
    fn test_cast_vote_against_and_abstain() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&v1, &300);
        client.set_voting_power(&v2, &200);
        advance(&env, 10);
        client.activate_proposal(&id);

        client.cast_vote(&v1, &id, &VoteChoice::Against);
        client.cast_vote(&v2, &id, &VoteChoice::Abstain);

        let p = client.get_proposal(&id);
        assert_eq!(p.votes_against, 300);
        assert_eq!(p.votes_abstain, 200);
    }

    #[test]
    #[should_panic(expected = "already voted")]
    fn test_double_vote_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &500);
        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);
        client.cast_vote(&voter, &id, &VoteChoice::Against);
    }

    #[test]
    #[should_panic(expected = "no voting power")]
    fn test_vote_with_zero_power_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);
    }

    #[test]
    #[should_panic(expected = "not active")]
    fn test_vote_on_pending_proposal_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &500);
        client.cast_vote(&voter, &id, &VoteChoice::For);
    }

    // ── quadratic voting ──────────────────────────────────────────────────────

    #[test]
    fn test_quadratic_voting() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, GovernanceContract);
        let client = GovernanceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        // initialize with quadratic enabled
        client.initialize(&admin, &100, &200, &10, &1000, &5100, &100, &true, &2);

        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        client.set_voting_power(&proposer, &1000);
        client.set_voting_power(&voter, &900); // sqrt(900) = 30

        let target = Address::generate(&env);
        let id = client.create_proposal(
            &proposer,
            &ProposalType::ParameterChange,
            &mk_str(&env, "t"),
            &mk_str(&env, "d"),
            &target,
            &mk_str(&env, "c"),
        );

        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);

        let p = client.get_proposal(&id);
        assert_eq!(p.votes_for, 30); // floor(sqrt(900)) = 30
    }

    // ── finalize_proposal ─────────────────────────────────────────────────────

    #[test]
    fn test_finalize_proposal_passed() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &1000);
        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);

        // total supply = 1000 + 1000 (proposer) = 2000
        // votes_for = 1000 → 50 % — but quorum is 10 % so met
        // threshold: 1000 / 1000 = 100 % > 51 % → passed
        advance(&env, 201);
        client.finalize_proposal(&id);
        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Passed);
    }

    #[test]
    fn test_finalize_proposal_failed_quorum() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        // proposer has 100 power but won't vote; add 10 000 to others to dilute
        let big_holder = Address::generate(&env);
        client.set_voting_power(&big_holder, &10_000);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        // no votes cast
        advance(&env, 10);
        client.activate_proposal(&id);
        advance(&env, 201);
        client.finalize_proposal(&id);

        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Failed);
    }

    #[test]
    fn test_finalize_proposal_failed_threshold() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let v_for = Address::generate(&env);
        let v_against = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&v_for, &400);
        client.set_voting_power(&v_against, &600);

        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&v_for, &id, &VoteChoice::For);
        client.cast_vote(&v_against, &id, &VoteChoice::Against);

        advance(&env, 201);
        client.finalize_proposal(&id);
        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Failed);
    }

    #[test]
    #[should_panic(expected = "voting still open")]
    fn test_finalize_before_end_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.finalize_proposal(&id); // too early
    }

    // ── queue_proposal ────────────────────────────────────────────────────────

    #[test]
    fn test_queue_proposal() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &1000);
        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);
        advance(&env, 201);
        client.finalize_proposal(&id);
        client.queue_proposal(&id);

        let p = client.get_proposal(&id);
        assert_eq!(p.status, ProposalStatus::Queued);
        assert!(p.execute_after > 0);
    }

    #[test]
    #[should_panic(expected = "not passed")]
    fn test_queue_failed_proposal_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        advance(&env, 201);
        client.finalize_proposal(&id);
        client.queue_proposal(&id); // failed, not passed
    }

    // ── execute_proposal ──────────────────────────────────────────────────────

    #[test]
    fn test_execute_proposal_after_timelock() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        let executor = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &1000);
        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);
        advance(&env, 201);
        client.finalize_proposal(&id);
        client.queue_proposal(&id);
        advance(&env, 100); // timelock_delay = 100

        client.execute_proposal(&executor, &id);
        let p = client.get_proposal(&id);
        assert_eq!(p.status, ProposalStatus::Executed);
        assert!(p.executed_at > 0);
    }

    #[test]
    #[should_panic(expected = "timelock not expired")]
    fn test_execute_before_timelock_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        let executor = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &1000);
        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);
        advance(&env, 201);
        client.finalize_proposal(&id);
        client.queue_proposal(&id);
        // no advance — timelock not elapsed
        client.execute_proposal(&executor, &id);
    }

    #[test]
    #[should_panic(expected = "not queued")]
    fn test_execute_non_queued_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let executor = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.execute_proposal(&executor, &id);
    }

    // ── cancel_proposal ───────────────────────────────────────────────────────

    #[test]
    fn test_cancel_by_proposer() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.cancel_proposal(&proposer, &id);
        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_by_admin() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.cancel_proposal(&admin, &id);
        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "unauthorised")]
    fn test_cancel_by_stranger_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let stranger = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.cancel_proposal(&stranger, &id);
    }

    #[test]
    #[should_panic(expected = "cannot cancel in current state")]
    fn test_cancel_executed_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        let executor = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &1000);
        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter, &id, &VoteChoice::For);
        advance(&env, 201);
        client.finalize_proposal(&id);
        client.queue_proposal(&id);
        advance(&env, 100);
        client.execute_proposal(&executor, &id);
        client.cancel_proposal(&proposer, &id);
    }

    // ── guardian execute ──────────────────────────────────────────────────────

    #[test]
    fn test_guardian_execute_emergency_proposal() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let g1 = Address::generate(&env);
        let g2 = Address::generate(&env);

        client.add_guardian(&g1);
        client.add_guardian(&g2);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::EmergencyPause,
        );
        advance(&env, 10);
        client.activate_proposal(&id);

        client.guardian_approve(&g1, &id);
        client.guardian_approve(&g2, &id);
        assert_eq!(client.get_guardian_approvals(&id), 2);

        client.guardian_execute(&g1, &id);
        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Executed);
    }

    #[test]
    #[should_panic(expected = "insufficient approvals")]
    fn test_guardian_execute_insufficient_approvals() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let g1 = Address::generate(&env);

        client.add_guardian(&g1);
        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::EmergencyPause,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.guardian_approve(&g1, &id);
        client.guardian_execute(&g1, &id); // threshold = 2, only 1 approval
    }

    #[test]
    #[should_panic(expected = "only emergency proposals")]
    fn test_guardian_execute_non_emergency_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let g1 = Address::generate(&env);
        let g2 = Address::generate(&env);

        client.add_guardian(&g1);
        client.add_guardian(&g2);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.guardian_approve(&g1, &id);
        client.guardian_approve(&g2, &id);
        client.guardian_execute(&g1, &id);
    }

    #[test]
    #[should_panic(expected = "not a guardian")]
    fn test_guardian_execute_non_guardian_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let stranger = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::EmergencyPause,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.guardian_execute(&stranger, &id);
    }

    #[test]
    #[should_panic(expected = "already approved")]
    fn test_double_guardian_approval_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let g1 = Address::generate(&env);

        client.add_guardian(&g1);
        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::EmergencyPause,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.guardian_approve(&g1, &id);
        client.guardian_approve(&g1, &id);
    }

    #[test]
    #[should_panic(expected = "not a guardian")]
    fn test_non_guardian_approve_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let stranger = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::EmergencyPause,
        );
        advance(&env, 10);
        client.activate_proposal(&id);
        client.guardian_approve(&stranger, &id);
    }

    // ── update_config ─────────────────────────────────────────────────────────

    #[test]
    fn test_update_config() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        client.update_config(&200, &400, &20, &2000, &6000, &200, &true, &3);
        let cfg = client.get_config();
        assert_eq!(cfg.timelock_delay, 200);
        assert_eq!(cfg.quorum_bps, 2000);
        assert_eq!(cfg.pass_threshold_bps, 6000);
        assert!(cfg.use_quadratic);
        assert_eq!(cfg.guardian_threshold, 3);
    }

    #[test]
    #[should_panic(expected = "quorum > 100%")]
    fn test_update_config_bad_quorum() {
        let (env, _admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        client.update_config(&200, &400, &20, &10_001, &6000, &200, &true, &3);
    }

    // ── proposal_count, get_vote on missing ───────────────────────────────────

    #[test]
    fn test_get_vote_returns_none_before_vote() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        assert!(client.get_vote(&id, &voter).is_none());
    }

    #[test]
    fn test_multiple_proposals() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        let id0 = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        let id1 = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::OperatorApproval,
        );
        let id2 = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::EmergencyPause,
        );

        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(client.proposal_count(), 3);
    }

    // ── isqrt edge cases ──────────────────────────────────────────────────────

    #[test]
    fn test_isqrt_values() {
        // via quadratic voting path
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, GovernanceContract);
        let client = GovernanceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin, &100, &200, &10, &1000, &5100, &100, &true, &2);

        let proposer = Address::generate(&env);
        let _voter0 = Address::generate(&env); // power = 0  (not used)
        let voter1 = Address::generate(&env); // power = 1  -> sqrt = 1
        let voter4 = Address::generate(&env); // power = 4  -> sqrt = 2
        let voter9 = Address::generate(&env); // power = 9  -> sqrt = 3

        client.set_voting_power(&proposer, &1000);
        client.set_voting_power(&voter1, &1);
        client.set_voting_power(&voter4, &4);
        client.set_voting_power(&voter9, &9);

        let target = Address::generate(&env);
        let id = client.create_proposal(
            &proposer,
            &ProposalType::ParameterChange,
            &mk_str(&env, "t"),
            &mk_str(&env, "d"),
            &target,
            &mk_str(&env, "c"),
        );

        advance(&env, 10);
        client.activate_proposal(&id);
        client.cast_vote(&voter1, &id, &VoteChoice::For);
        client.cast_vote(&voter4, &id, &VoteChoice::For);
        client.cast_vote(&voter9, &id, &VoteChoice::For);

        let p = client.get_proposal(&id);
        assert_eq!(p.votes_for, 1 + 2 + 3); // 6
    }

    // ── outside voting window ─────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "outside voting window")]
    fn test_vote_after_end_time_panics() {
        let (env, admin, contract_id) = setup();
        let client = GovernanceContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let id = create_funded_proposal(
            &env,
            &client,
            &proposer,
            &admin,
            ProposalType::ParameterChange,
        );
        client.set_voting_power(&voter, &500);
        advance(&env, 10);
        client.activate_proposal(&id);
        advance(&env, 300); // past end_time (voting_period = 200)
        client.cast_vote(&voter, &id, &VoteChoice::For);
    }
}
