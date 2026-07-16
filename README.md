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

## Related repositories

- [`swipely_frontend`](https://github.com/stellar-kracken/swipely_frontend) — dashboard UI
- [`swipely_backend`](https://github.com/stellar-kracken/swipely_backend) — API and monitoring services

## License

MIT — see [`LICENSE`](./LICENSE).
