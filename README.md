# Swipely — Smart Contracts

[![CI](https://github.com/stellar-kracken/swipely_contract/actions/workflows/ci.yml/badge.svg)](https://github.com/stellar-kracken/swipely_contract/actions/workflows/ci.yml)

**Soroban** smart contracts for **Swipely**, the cross-chain bridge and DEX
liquidity monitoring platform on the Stellar network. These contracts provide the
on-chain primitives the platform relies on — trusted source registries, asset
locking/escrow, operator rotation, and transfer state tracking.

## Workspace layout

This is a Cargo workspace with the following members:

| Crate | Description |
| --- | --- |
| `soroban/` | Core Soroban contracts (access control, trusted sources, thresholds) |
| `escrow_contract/` | Time-locked escrow contract for bridge transfers |
| `transfer_state_machine/` | Transfer state-machine contract logic |
| `harness/` | Test harness and integration helpers |

## Prerequisites

- **Rust** (stable) with the `wasm32-unknown-unknown` target
- **Soroban CLI** (`stellar` / `soroban`)

```bash
rustup target add wasm32-unknown-unknown
```

## Build & test

```bash
# Build optimized wasm for release
cargo build --release --target wasm32-unknown-unknown

# Run the contract test suites
cargo test
```

The release profile is tuned for small wasm output (`opt-level = "z"`, LTO,
symbol stripping) — see [`Cargo.toml`](./Cargo.toml).

## Contract API reference

Every public function under `soroban/src/` — arguments, authorization
requirements, and emitted events — is documented in
[`soroban/docs/API_REFERENCE.md`](./soroban/docs/API_REFERENCE.md), organized by
contract and grouped by what's actually compiled into the deployed contract
versus test-only or unreferenced code. The corresponding numeric error codes
are consolidated in [`soroban/docs/ERRORS.md`](./soroban/docs/ERRORS.md),
along with a check that each contract's codes are internally unique.

## Benchmarks

Resource-usage (CPU instructions, memory, ledger footprint) benchmarks for
the escrow, threshold-window, and source-trust hot paths live in
[`BENCHMARKS.md`](./BENCHMARKS.md), which documents how to re-run them and
records baseline numbers to compare against.

## Code style

Formatting and lints are enforced across the workspace and run as their own
CI job:

```bash
# Formatting (settings in rustfmt.toml)
cargo fmt --all -- --check

# Lints, with warnings treated as errors
cargo clippy --all-targets --all-features -- -D warnings
```

Run `cargo fmt --all` to format locally before committing. Where a lint is
intentionally suppressed (e.g. a public contract entrypoint's argument count,
or a reserved-for-later constant), the `#[allow(...)]` is accompanied by a
comment explaining why.

## Related repositories

- [`swipely_frontend`](https://github.com/stellar-kracken/swipely_frontend) — dashboard UI
- [`swipely_backend`](https://github.com/stellar-kracken/swipely_backend) — API and monitoring services

## License

MIT — see [`LICENSE`](./LICENSE).
