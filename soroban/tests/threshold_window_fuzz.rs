#![cfg(test)]
// Deterministic, seed-driven fuzz coverage for `threshold_window`, mirroring the
// pattern established in `relay_contract_fuzz.rs`. `threshold_window`'s functions
// are plain (non-contract) entrypoints that validate input via `panic!`, so unlike
// the relay contract's `try_*` client methods there is no host-level boundary to
// catch a bad panic — we use `catch_unwind` and assert every panic message is one
// of the module's known validation messages. Any panic outside that allow-list
// is treated as a bug.
//
// Iteration count is capped by default so this stays cheap in CI; set
// FUZZ_ITERATIONS to run a longer campaign locally, e.g.:
//   FUZZ_ITERATIONS=5000 cargo test --test threshold_window_fuzz

use std::panic::{self, AssertUnwindSafe};

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, Env, String,
};
use swipely_contracts::threshold_window::{
    create_window, evaluate_threshold, get_all_windows, get_window, remove_window, WindowUnit,
    MAX_WINDOWS,
};

const DEFAULT_FUZZ_ITERATIONS: u64 = 40;

const KNOWN_PANIC_MESSAGES: &[&str] = &[
    "window_id cannot be empty",
    "window length must be greater than 0",
    "threshold must be between 1 and 10_000 bps",
    "window already exists",
    "maximum number of windows reached",
];

fn fuzz_iterations() -> u64 {
    std::env::var("FUZZ_ITERATIONS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_FUZZ_ITERATIONS)
}

// Minimal test contract — needed so env.as_contract() can provide a storage context.
#[contract]
struct TestContract;
#[contractimpl]
impl TestContract {}

// "admin" mirrors the private keys::ADMIN constant value ("admin").
fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(TestContract, ());
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&"admin", &admin);
    });
    env.ledger().set_timestamp(1_000_000);
    (env, admin, contract_id)
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

type StdString = std::string::String;

fn panic_message(payload: &(dyn std::any::Any + Send)) -> StdString {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<StdString>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[test]
fn fuzz_create_window_validates_inputs_without_unexpected_panics() {
    let mut created = 0_u32;
    let mut rejected_empty_id = 0_u32;
    let mut rejected_zero_length = 0_u32;
    let mut rejected_bad_threshold = 0_u32;

    for i in 0..fuzz_iterations() {
        // Fresh env per iteration keeps each trial independent of MAX_WINDOWS.
        let (env, admin, contract_id) = setup();
        let mut rng = Rng::new(i.wrapping_add(1));

        let kind = rng.next_u64() % 4;
        let (window_id, length, threshold_bps) = match kind {
            0 => (
                String::from_str(&env, ""),
                rng.next_range(1, 1_000_000),
                500u32,
            ),
            1 => (
                String::from_str(&env, "win"),
                0u64,
                rng.next_range(1, 10_000) as u32,
            ),
            2 => {
                let bad = if rng.next_u64().is_multiple_of(2) {
                    0u32
                } else {
                    10_001u32
                };
                (
                    String::from_str(&env, "win"),
                    rng.next_range(1, 1_000_000),
                    bad,
                )
            }
            _ => (
                String::from_str(&env, "win"),
                rng.next_range(1, 1_000_000),
                rng.next_range(1, 10_000) as u32,
            ),
        };

        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            env.as_contract(&contract_id, || {
                create_window(
                    &env,
                    &admin,
                    window_id.clone(),
                    length,
                    WindowUnit::Hours,
                    threshold_bps,
                );
            });
        }));

        match outcome {
            Ok(()) => {
                created += 1;
                env.as_contract(&contract_id, || {
                    let stored = get_window(&env, &window_id).expect("just-created window");
                    assert!(stored.length > 0);
                    assert!(stored.threshold_bps > 0 && stored.threshold_bps <= 10_000);
                });
            }
            Err(payload) => {
                let message = panic_message(&*payload);
                assert!(
                    KNOWN_PANIC_MESSAGES.contains(&message.as_str()),
                    "unexpected panic from create_window (possible bug): {message}"
                );
                match message.as_str() {
                    "window_id cannot be empty" => rejected_empty_id += 1,
                    "window length must be greater than 0" => rejected_zero_length += 1,
                    "threshold must be between 1 and 10_000 bps" => rejected_bad_threshold += 1,
                    _ => {}
                }
            }
        }
    }

    assert!(created > 0, "expected at least one accepted window");
    assert!(
        rejected_empty_id > 0,
        "expected empty-id rejection coverage"
    );
    assert!(
        rejected_zero_length > 0,
        "expected zero-length rejection coverage"
    );
    assert!(
        rejected_bad_threshold > 0,
        "expected bad-threshold rejection coverage"
    );

    println!(
        "fuzz summary: created={created}, rejected_empty_id={rejected_empty_id}, \
         rejected_zero_length={rejected_zero_length}, rejected_bad_threshold={rejected_bad_threshold}"
    );
}

#[test]
fn fuzz_evaluate_threshold_matches_expected_deviation_and_never_panics() {
    let (env, admin, contract_id) = setup();
    let window_id = String::from_str(&env, "fuzz-window");

    env.as_contract(&contract_id, || {
        create_window(&env, &admin, window_id.clone(), 1, WindowUnit::Hours, 500);
    });

    let mut breached = 0_u32;
    let mut not_breached = 0_u32;

    for i in 0..fuzz_iterations() {
        let mut rng = Rng::new(i.wrapping_add(7));
        // Bounded magnitude (including negative values) keeps `diff * 10_000`
        // well inside i128 range so the module's own (unchecked) arithmetic
        // can't overflow on our inputs, while still covering both signs.
        const BOUND: i128 = 1_000_000_000_000;
        let reference_value: i128 = if i.is_multiple_of(5) {
            0
        } else {
            let v = rng.next_range(0, 2 * BOUND as u64) as i128 - BOUND;
            if v == 0 {
                1
            } else {
                v
            }
        };
        let current_value: i128 = rng.next_range(0, 2 * BOUND as u64) as i128 - BOUND;

        let eval = env.as_contract(&contract_id, || {
            evaluate_threshold(&env, &window_id, reference_value, current_value)
        });
        let eval = eval.expect("window exists");

        assert_eq!(eval.value, current_value);
        assert_eq!(eval.threshold_bps, 500);

        if reference_value == 0 {
            assert!(!eval.is_breached);
            assert_eq!(eval.breach_bps, 0);
        } else {
            let diff = if current_value > reference_value {
                current_value - reference_value
            } else {
                reference_value - current_value
            };
            let expected_bps = diff * 10_000 / reference_value;
            assert_eq!(eval.breach_bps, expected_bps);
            assert_eq!(eval.is_breached, expected_bps > 500);
        }

        if eval.is_breached {
            breached += 1;
        } else {
            not_breached += 1;
        }
    }

    assert!(breached > 0, "expected at least one breach classification");
    assert!(
        not_breached > 0,
        "expected at least one non-breach classification"
    );

    println!("fuzz summary: breached={breached}, not_breached={not_breached}");
}

#[test]
fn fuzz_max_windows_cap_and_removal_keep_state_consistent() {
    let rounds = fuzz_iterations().min(20);

    for i in 0..rounds {
        let (env, admin, contract_id) = setup();
        let mut rng = Rng::new(i.wrapping_add(31));

        for w in 0..MAX_WINDOWS {
            let id = String::from_str(&env, window_name(w));
            env.as_contract(&contract_id, || {
                create_window(&env, &admin, id, 1, WindowUnit::Hours, 500);
            });
        }

        env.as_contract(&contract_id, || {
            assert_eq!(get_all_windows(&env).len(), MAX_WINDOWS);
        });

        let overflow_id = String::from_str(&env, "overflow");
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            env.as_contract(&contract_id, || {
                create_window(&env, &admin, overflow_id, 1, WindowUnit::Hours, 500);
            });
        }));
        match outcome {
            Ok(()) => panic!("expected MAX_WINDOWS cap to reject an additional window"),
            Err(payload) => {
                let message = panic_message(&*payload);
                assert_eq!(message, "maximum number of windows reached");
            }
        }

        // Remove a pseudo-random subset and verify the list stays consistent.
        let mut remaining = MAX_WINDOWS;
        for w in 0..MAX_WINDOWS {
            if rng.next_u64().is_multiple_of(2) {
                let id = String::from_str(&env, window_name(w));
                env.as_contract(&contract_id, || {
                    remove_window(&env, &admin, id.clone());
                    assert!(get_window(&env, &id).is_none());
                });
                remaining -= 1;
            }
        }

        env.as_contract(&contract_id, || {
            assert_eq!(get_all_windows(&env).len(), remaining);
        });
    }
}

fn window_name(index: u32) -> &'static str {
    const NAMES: [&str; 10] = [
        "win-0", "win-1", "win-2", "win-3", "win-4", "win-5", "win-6", "win-7", "win-8", "win-9",
    ];
    NAMES[index as usize]
}
