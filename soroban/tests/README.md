# Soroban Contract Testing Suite

This directory contains external integration-style tests for Soroban contracts.

## Scope

The suite complements in-module tests in `src/` and focuses on:

- full message lifecycle integration paths
- access-control and authorization-sensitive paths
- error-condition and edge-case regressions
- event-emission assertions
- state-transition correctness
- fee/gas regression proxies for optimization checks

## Current files

- `relay_contract.integration.rs`: Cross-chain relay lifecycle, batch paths, events, and failure modes.

## Patterns used

1. Build deterministic test fixtures with `setup_context()`.
2. Prefer `try_*` client methods for explicit error assertions.
3. Assert both return values and persisted state (`get_message`, `get_metrics`, queue length).
4. Validate critical event topics with `env.events().all()`.
5. Include boundary-value checks (max payload, TTL expiry, batch partial failure).

## Running contract tests

From `contracts/`:

```bash
cargo test -p bridge-watch-contracts
```

Or run all workspace tests:

```bash
cargo test --workspace
```

## Coverage notes

- Existing in-module tests in `src/lib.rs` and `src/relay/mod.rs` provide broad unit-level coverage.
- External tests in this directory emphasize integration behavior and regression safety across public contract clients.
