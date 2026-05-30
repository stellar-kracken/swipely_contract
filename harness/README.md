# Contract Scenario Harness

This harness provides a reusable framework for exercising contract lifecycle flows in a local, deterministic environment.

## Overview

The harness is designed to:
- Automate contract builds and deployments.
- Reset network state between runs.
- Allow for easy invocation of contract functions.
- Provide clear, readable outputs for scenario steps.
- Be run from the command line.

## Prerequisites

- [Soroban CLI](https://soroban.stellar.org/docs/getting-started/setup) installed and configured.
- A local Soroban network running.

## Usage

1.  **Build Contracts**: The harness automatically builds the contracts before running a scenario.
2.  **Run a Scenario**: Execute a scenario script from the `contracts/harness/scenarios` directory.

    ```bash
    node contracts/harness/scenarios/transfer-lifecycle.js
    ```

## Key Components

-   `contracts/harness/scenario-utils.js`: A set of helper functions for building, deploying, and interacting with contracts.
-   `contracts/harness/scenarios/`: Directory containing individual scenario scripts.

## Creating a New Scenario

1.  Create a new JavaScript file in `contracts/harness/scenarios`.
2.  Import the utilities from `scenario-utils.js`.
3.  Define your scenario steps:
    -   Call `resetState()` at the beginning.
    -   Call `buildContracts()` to ensure contracts are up-to-date.
    -   Use `deployContract()` to deploy your contract and get its ID.
    -   Use `invokeContract()` to call functions on the deployed contract.
    -   Add assertions to verify the contract's state at different points.
4.  Run your new scenario script from the root of the repository.
