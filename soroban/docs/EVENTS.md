# Swipely Smart Contract Events Standard

This document details the standardized event emission patterns used across the Swipely Soroban smart contracts.

## Overview

To enable reliable off-chain indexing and integration (e.g., in event_query.rs), all events across the Swipely ecosystem follow a strict specification.

### 1. Topic Hierarchy
All events emit a 4-tuple of topics to allow granular filtering:

```rust
(
    symbol_short!("Swipely"),       // [0] Ecosystem Root
    Symbol::new(&env, "Module"),    // [1] Contract/Module Name
    Symbol::new(&env, "EventName"), // [2] Specific Event
    indexed_field,                  // [3] Primary ID (e.g. Address, proposal_id, rule_id)
)
```

### 2. Versioned Payloads
Every event payload is a strict `#[contracttype]` struct. To ensure backwards compatibility as the protocol upgrades, every event struct MUST begin with a `version: u32` field (currently `1`).

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExampleEvent {
    pub version: u32, // Always 1 for v1
    // ... fields
}
```

## Catalog of Events

### Alert System (`soroban/src/alert_system.rs`)
Module Topic: `"AlertSys"`
- `RuleRegisteredEvent`: Emitted when a new alert rule is registered. Indexed by `rule_id`.
- `RuleUpdatedEvent`: Emitted when a rule configuration changes. Indexed by `rule_id`.
- `RuleStatusChangedEvent`: Emitted when a rule's active status toggles. Indexed by `rule_id`.
- `AssetEvaluatedEvent`: Emitted when an asset evaluation completes. Indexed by `asset`.

### Escrow Contract (`escrow_contract/src/lib.rs`)
Module Topic: `"Escrow"`
- `DepositEvent`: Emitted when an asset is deposited. Indexed by `user`.
- `WithdrawEvent`: Emitted when an asset is withdrawn. Indexed by `user`.
- `FeeUpdatedEvent`: Emitted when the global fee changes. Indexed by `admin` (or dummy symbol `"global"`).

### Operator Rotation (`soroban/src/operator_rotation.rs`)
Module Topic: `"Operator"`
- `OperatorAddedEvent`: Emitted when an operator is added. Indexed by `operator` address.
- `OperatorRemovedEvent`: Emitted when an operator is removed. Indexed by `operator` address.

### Governance (`soroban/src/governance.rs`)
Module Topic: `"Governance"`
- `ConfigUpdatedEvent`: Emitted when the global config initializes or updates entirely. Indexed by `sys`.
- `ParameterChangedEvent`: Emitted for specific parameter changes (quorum, delay, etc). Indexed by `sys`.
- `ProposalCreatedEvent`: Emitted when a proposal is created. Indexed by `proposal_id`.
- `ProposalStatusChangedEvent`: Emitted when a proposal transitions state (Active, Queued, Cancelled). Indexed by `proposal_id`.
- `VoteCastEvent`: Emitted when a voter casts a vote. Indexed by `proposal_id`.
- `ProposalExecutedEvent`: Emitted when a proposal successfully executes (regular or emergency). Indexed by `proposal_id`.
