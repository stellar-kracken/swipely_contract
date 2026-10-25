# WASM release binary size budgets

Every Swipely on-chain contract is built in CI by the existing release command:

```bash
cargo build --locked --release --target wasm32-unknown-unknown
python3 scripts/check_wasm_size.py
```

The size gate reads `scripts/wasm-size-budgets.json`, measures **all**
`target/wasm32-unknown-unknown/release/*.wasm` artifacts in bytes, and prints
an actual/baseline/limit/delta summary. CI **fails** for an oversized contract,
for a contract omitted from the budget file, or for a missing expected artifact.
Each run also publishes a table in the GitHub Actions job summary.

## Committed baselines and thresholds

These are measured **release** sizes from the unmodified main-branch source at
commit `c15826d5f2e18422a5b1bb21c31d1ccd4e211045`, using the repository's pinned Rust
**1.97.1**, the workspace release profile in `Cargo.toml`, and
`wasm32-unknown-unknown`. The size budget gives each binary **10% headroom,
rounded up to the next 1 KiB**, and is committed in JSON so reviewers can
see when it changes. This is regression protection, not a Soroban chain limit.

| Artifact | Measured baseline | CI budget | Headroom |
| --- | ---: | ---: | ---: |
| `escrow_contract.wasm` | 49,810 B | 55,296 B | 5,486 B |
| `swipely_contracts.wasm` | 369,541 B | 406,528 B | 36,987 B |
| `transfer_state_machine.wasm` | 18,919 B | 21,504 B | 2,585 B |

## Updating a budget intentionally

1. Use the pinned Rust toolchain and build with the same command as CI. For
   consistent comparisons, do not mix unoptimized debug WASM with release
   binaries, and use the same workspace release profile.
2. Check the cause of any size increase (features, dependencies, inlining,
   optimization profile) before allowing it.
3. Record the new baseline from the actual `.wasm` file size, then compute the
   new budget with `ceil(baseline * 1.10 / 1024) * 1024`. Make the manifest
   change explicit in your PR with the reason for the increase.
4. Re-run `python3 scripts/check_wasm_size.py` after the release build and
   `python3 -m unittest discover -s scripts/tests -p 'test_*.py'`. The test
   suite checks both passing and failing budgets without compiling Rust.

The budget checker requires only Python 3 (stdlib) on the GitHub Linux runner;
it does not need dependencies or write to the built WASM binaries.
