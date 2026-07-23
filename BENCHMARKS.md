# Contract Benchmarks

Resource-usage benchmarks for the workspace's hottest contract entry points:
escrow lock/release, threshold checks, and source trust updates.

## Running the benchmarks

Each benchmark is a normal `#[test]` that performs exactly one top-level
contract invocation and prints the resources Soroban's test host metered for
it. Run with `--nocapture` to see the report; without it, cargo hides
`println!` output for passing tests.

```bash
# Escrow lock/release
cargo test -p escrow-contract --test benchmarks -- --nocapture

# Threshold checks and source trust updates
cargo test -p swipely-contracts --test benchmarks -- --nocapture
```

Each test also asserts that instructions are non-zero, so a broken/absent
metering setup fails loudly instead of silently printing zeros.

## Interpreting the numbers

Each line reports, for a single top-level call:

| Column | Meaning |
| --- | --- |
| `instructions` | Modelled CPU instructions consumed |
| `mem_bytes` | Modelled memory used |
| `read_entries` / `write_entries` | Ledger entries read / written |
| `read_bytes` / `write_bytes` | Total bytes read / written across those entries |
| `est_fee_stroops` | Rough fee estimate (instructions + reads/writes + rent), using a 2024-12-11 Pubnet fee snapshot |

These come from `soroban_sdk`'s `Env::cost_estimate()` test utility
(`resources()` and `fee()`), which is the resource/cost data source the SDK
exposes directly to tests — see the [`cost_estimate`
docs](https://docs.rs/soroban-sdk) for details.

**Caveats:**
- The tests run the contract as native Rust, not compiled Wasm, so
  instruction/memory counts are known to be *underestimates* relative to a
  real Wasm invocation (VM instantiation and Wasm-level costs aren't
  modelled). Ledger footprint (read/write entries and bytes) is accurate
  regardless, since it reflects actual storage access.
- Treat these as a **relative, repeatable baseline for regression
  comparisons** (did a change make this entry point noticeably more
  expensive?), not as an absolute on-chain fee prediction. For that, use
  `soroban-cli`/RPC simulation against a real Wasm build.

## Baseline

Captured on `rustc 1.97.1`, `soroban-sdk 22.0.11`, workspace commit at the
time this file was added. Re-run the commands above to compare against a
change.

### Escrow (`escrow_contract`)

| Entry point | Instructions | Memory (bytes) | Read entries | Write entries | Read bytes | Write bytes | Est. fee (stroops) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `create_escrow` (lock) | 142,544 | 19,936 | 1 | 2 | 596 | 1,312 | 1,360,071 |
| `release_escrow` (release) | 173,554 | 30,150 | 1 | 2 | 1,240 | 1,312 | 1,335,192 |

### Threshold windows (`soroban::threshold_window`)

| Entry point | Instructions | Memory (bytes) | Read entries | Write entries | Read bytes | Write bytes | Est. fee (stroops) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `create_window` | 66,782 | 9,729 | 2 | 3 | 164 | 512 | 1,376,832 |
| `evaluate_threshold` | 21,203 | 2,953 | 2 | 0 | 484 | 0 | 13,399 |

### Source trust (`soroban::source_trust`, via `BridgeWatchContract`)

| Entry point | Instructions | Memory (bytes) | Read entries | Write entries | Read bytes | Write bytes | Est. fee (stroops) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `register_trusted_source` | 211,519 | 32,820 | 2 | 3 | 1,772 | 624 | 1,385,391 |
| `revoke_trusted_source` | 210,248 | 32,569 | 2 | 2 | 2,184 | 528 | 1,346,187 |

`evaluate_threshold` is a read-only check and is markedly cheaper than the
write-heavy `create_window`/`register_trusted_source`/`create_escrow` paths,
as expected — ledger writes dominate both instructions and estimated fee.
