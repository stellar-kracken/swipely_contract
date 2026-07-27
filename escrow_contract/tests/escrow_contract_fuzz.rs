#![cfg(test)]
// Deterministic, seed-driven fuzz coverage for the time-locked escrow contract,
// mirroring the pattern in `soroban/tests/relay_contract_fuzz.rs`. All calls go
// through `try_*` client methods, so a host-level panic surfaces as `Err(Err(_))`
// instead of aborting the test process — treated below as a bug, not a pass/fail
// classification.
//
// Invariants checked: released/refunded amounts never exceed what was deposited
// (no negative balances), a finalized escrow can never be released or refunded
// again (no invalid state transitions), and arbitrary/edge-case input is always
// classified into a known `EscrowError`, never an unexpected host error.
//
// Iteration count is capped by default so this stays cheap in CI; set
// FUZZ_ITERATIONS to run a longer campaign locally, e.g.:
//   FUZZ_ITERATIONS=5000 cargo test --test escrow_contract_fuzz

use escrow_contract::{EscrowError, TimeLockedEscrowContract, TimeLockedEscrowContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

const DEFAULT_FUZZ_ITERATIONS: u64 = 40;

fn fuzz_iterations() -> u64 {
    std::env::var("FUZZ_ITERATIONS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_FUZZ_ITERATIONS)
}

// Small xorshift64 PRNG so each iteration derives several independent values
// from a single seed, without pulling in an external RNG crate.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi - lo + 1)
    }
}

fn setup() -> (
    Env,
    TimeLockedEscrowContractClient<'static>,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register(TimeLockedEscrowContract, ());
    let client = TimeLockedEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let approver_1 = Address::generate(&env);
    let approver_2 = Address::generate(&env);

    let mut approvers = Vec::new(&env);
    approvers.push_back(approver_1);
    approvers.push_back(approver_2);

    client.initialize(&admin, &fee_collector, &100u32, &approvers, &2u32);

    (env, client, admin, fee_collector)
}

#[test]
fn fuzz_create_escrow_classifies_invalid_amount_and_metadata_without_panicking() {
    let (env, client, _admin, _fee_collector) = setup();
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let verifier = Address::generate(&env);
    let bridge = String::from_str(&env, "stellar-eth");
    let asset = String::from_str(&env, "USDC");
    let ver_ref = String::from_str(&env, "ref");

    let mut created = 0_u32;
    let mut rejected_invalid_amount = 0_u32;
    let mut rejected_metadata_too_large = 0_u32;

    for i in 0..fuzz_iterations() {
        let mut rng = Rng::new(i.wrapping_add(1));
        // 0/1/2 -> deliberately invalid amount (zero, negative, overflow-prone
        // max) paired with valid metadata; 3 -> valid amount with oversized
        // metadata; 4 -> fully valid input that should be accepted.
        let kind = rng.next_u64() % 5;
        let (amount, metadata_len): (i128, usize) = match kind {
            0 => (0, rng.next_range(0, 512) as usize),
            1 => (
                -(rng.next_range(1, 1_000_000_000) as i128),
                rng.next_range(0, 512) as usize,
            ),
            2 => (i128::MAX, rng.next_range(0, 512) as usize),
            3 => (
                rng.next_range(1, 1_000_000_000) as i128,
                513 + rng.next_range(0, 500) as usize,
            ),
            _ => (
                rng.next_range(1, 1_000_000_000) as i128,
                rng.next_range(0, 512) as usize,
            ),
        };
        let metadata = String::from_str(&env, &"x".repeat(metadata_len));

        let result = client.try_create_escrow(
            &depositor, &recipient, &bridge, &asset, &amount, &metadata, &verifier, &ver_ref,
        );

        match result {
            Ok(Ok(escrow_id)) => {
                assert_eq!(
                    kind, 4,
                    "unexpected success for deliberately invalid input (kind {kind})"
                );
                created += 1;
                let escrow = client.get_escrow(&escrow_id).expect("just-created escrow");
                assert!(escrow.amount > 0);
                assert!(escrow.fee_total >= 0 && escrow.fee_total <= escrow.amount);
                assert_eq!(escrow.released_amount, 0);
            }
            Ok(Err(_)) => panic!("unexpected value-conversion error for kind {kind}"),
            Err(Ok(EscrowError::InvalidAmount)) => {
                assert!(kind <= 2, "unexpected InvalidAmount for kind {kind}");
                rejected_invalid_amount += 1;
            }
            Err(Ok(EscrowError::MetadataTooLarge)) => {
                assert_eq!(kind, 3, "unexpected MetadataTooLarge for kind {kind}");
                rejected_metadata_too_large += 1;
            }
            Err(Ok(other)) => {
                panic!("kind {kind} produced unexpected contract error {other:?}")
            }
            Err(Err(_)) => {
                panic!("unexpected host-level panic/error during create_escrow for kind {kind} (possible bug)")
            }
        }
    }

    assert!(created > 0, "expected at least one accepted escrow");
    assert!(
        rejected_invalid_amount > 0,
        "expected invalid-amount rejection coverage"
    );
    assert!(
        rejected_metadata_too_large > 0,
        "expected metadata-too-large rejection coverage"
    );

    println!(
        "fuzz summary: created={created}, rejected_invalid_amount={rejected_invalid_amount}, \
         rejected_metadata_too_large={rejected_metadata_too_large}"
    );
}

#[test]
fn fuzz_release_never_exceeds_balance_and_blocks_after_finalization() {
    let (env, client, _admin, _fee_collector) = setup();
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let verifier = Address::generate(&env);

    // Sized against the iteration count so the sum of every "kind 3" (valid,
    // <=5_000) release across the whole loop can never exhaust the escrow —
    // that keeps every rejection in the loop attributable to the amount
    // itself, regardless of how long a local campaign (FUZZ_ITERATIONS) runs.
    let amount: i128 = 1_000_000 + fuzz_iterations() as i128 * 5_000;
    let escrow_id = client.create_escrow(
        &depositor,
        &recipient,
        &String::from_str(&env, "bridge-fuzz"),
        &String::from_str(&env, "USDC"),
        &amount,
        &String::from_str(&env, "meta"),
        &verifier,
        &String::from_str(&env, "proof"),
    );
    client.sync_verification(&verifier, &escrow_id, &true);
    // Well past the default 3_600s lock period.
    env.ledger().with_mut(|li| li.timestamp += 10_000);

    let releasable_total = {
        let escrow = client.get_escrow(&escrow_id).unwrap();
        escrow.amount - escrow.fee_total
    };

    let mut successful_releases = 0_u32;
    let mut rejected_releases = 0_u32;

    for i in 0..fuzz_iterations() {
        let mut rng = Rng::new(i.wrapping_add(1));
        // Sums of the "kind 3" branch stay far below `releasable_total`, so the
        // escrow is guaranteed to remain Pending for the whole loop and every
        // rejection below is attributable to the amount itself.
        let kind = rng.next_u64() % 4;
        let release_amount: i128 = match kind {
            0 => 0,
            1 => -(rng.next_range(1, 1_000) as i128),
            2 => releasable_total + 1 + rng.next_range(0, 1_000) as i128,
            _ => rng.next_range(1, 5_000) as i128,
        };

        let before = client.get_escrow(&escrow_id).unwrap().released_amount;
        match client.try_release_escrow(&recipient, &escrow_id, &release_amount) {
            Ok(Ok(released)) => {
                assert_eq!(
                    kind, 3,
                    "unexpected success for a deliberately invalid amount"
                );
                successful_releases += 1;
                assert_eq!(released, release_amount);
            }
            Ok(Err(_)) => panic!("unexpected value-conversion error for kind {kind}"),
            Err(Ok(EscrowError::InvalidAmount)) => {
                assert!(
                    kind <= 2,
                    "unexpected InvalidAmount for a valid small release"
                );
                rejected_releases += 1;
            }
            Err(Ok(other)) => panic!("unexpected contract error {other:?} for kind {kind}"),
            Err(Err(_)) => {
                panic!("unexpected host-level panic/error on release_escrow (possible bug)")
            }
        }

        let after = client.get_escrow(&escrow_id).unwrap().released_amount;
        assert!(after >= before, "released_amount must never decrease");
        assert!(
            after >= 0 && after <= releasable_total,
            "released_amount out of bounds"
        );
    }

    assert!(
        successful_releases > 0,
        "expected at least one successful release"
    );
    assert!(
        rejected_releases > 0,
        "expected at least one rejected release"
    );

    // Deterministic tail: drain the remainder, then confirm neither a further
    // release nor a refund can succeed once the escrow is fully released.
    let remaining = releasable_total - client.get_escrow(&escrow_id).unwrap().released_amount;
    if remaining > 0 {
        assert!(client
            .try_release_escrow(&recipient, &escrow_id, &remaining)
            .is_ok());
    }
    assert_eq!(
        client.get_escrow(&escrow_id).unwrap().released_amount,
        releasable_total
    );
    assert!(client
        .try_release_escrow(&recipient, &escrow_id, &1)
        .is_err());
    assert!(client.try_refund_escrow(&depositor, &escrow_id).is_err());

    println!(
        "fuzz summary: successful_releases={successful_releases}, rejected_releases={rejected_releases}"
    );
}

#[test]
fn fuzz_batch_release_keeps_uniform_escrows_in_lockstep_without_overdraw() {
    let (env, client, _admin, _fee_collector) = setup();
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let verifier = Address::generate(&env);

    let mut ids = Vec::new(&env);
    for _ in 0..4 {
        let escrow_id = client.create_escrow(
            &depositor,
            &recipient,
            &String::from_str(&env, "bridge-batch"),
            &String::from_str(&env, "USDC"),
            &100_000i128,
            &String::from_str(&env, "meta"),
            &verifier,
            &String::from_str(&env, "proof"),
        );
        client.sync_verification(&verifier, &escrow_id, &true);
        ids.push_back(escrow_id);
    }
    env.ledger().with_mut(|li| li.timestamp += 10_000);

    let releasable_total_each = {
        let escrow = client.get_escrow(&ids.get(0).unwrap()).unwrap();
        escrow.amount - escrow.fee_total
    };

    let mut successful_batches = 0_u32;
    let mut rejected_batches = 0_u32;

    for i in 0..fuzz_iterations() {
        let mut rng = Rng::new(i.wrapping_add(1));
        let release_amount = rng.next_range(1, 2_000) as i128;
        let remaining_before = releasable_total_each
            - client
                .get_escrow(&ids.get(0).unwrap())
                .unwrap()
                .released_amount;
        let should_succeed = release_amount <= remaining_before;

        match client.try_batch_release(&recipient, &ids, &release_amount) {
            Ok(Ok(released)) => {
                assert!(
                    should_succeed,
                    "batch succeeded despite exceeding remaining balance"
                );
                successful_batches += 1;
                assert_eq!(released.len(), ids.len());
                for r in released.iter() {
                    assert_eq!(r, release_amount);
                }
            }
            Ok(Err(_)) => panic!("unexpected value-conversion error on batch_release"),
            Err(Ok(_)) => {
                assert!(
                    !should_succeed,
                    "batch rejected despite sufficient remaining balance"
                );
                rejected_batches += 1;
            }
            Err(Err(_)) => {
                panic!("unexpected host-level panic/error on batch_release (possible bug)")
            }
        }

        // Uniform escrows must stay in lockstep: identical released_amount,
        // never above what a single escrow can hold.
        let mut last: Option<i128> = None;
        for id in ids.iter() {
            let escrow = client.get_escrow(&id).unwrap();
            assert!(escrow.released_amount >= 0);
            assert!(escrow.released_amount <= releasable_total_each);
            if let Some(prev) = last {
                assert_eq!(prev, escrow.released_amount);
            }
            last = Some(escrow.released_amount);
        }
    }

    assert!(
        successful_batches > 0,
        "expected at least one successful batch"
    );

    println!(
        "fuzz summary: successful_batches={successful_batches}, rejected_batches={rejected_batches}"
    );
}
