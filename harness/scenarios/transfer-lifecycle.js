const {
  buildContracts,
  deployContract,
  invokeContract,
  resetState,
} = require("./scenario-utils");

function runTransferScenario() {
  console.log("\n--- Running Transfer Lifecycle Scenario ---");

  // 1. Reset state
  resetState();

  // 2. Build contracts
  buildContracts();

  // 3. Deploy the transfer state machine contract
  const transferContractId = deployContract("transfer_state_machine");

  // 4. Initialize the transfer
  const sender = "G..."; // Replace with a valid sender address
  const recipient = "G..."; // Replace with a valid recipient address
  const asset = "G..."; // Replace with a valid asset address
  const amount = 1000;

  invokeContract(transferContractId, "init", {
    sender,
    recipient,
    asset,
    amount,
  });
  console.log("Transfer initialized.");

  // 5. Lock the transfer
  invokeContract(transferContractId, "lock");
  console.log("Transfer locked.");

  // 6. Confirm the transfer
  invokeContract(transferContractId, "confirm");
  console.log("Transfer confirmed.");

  // 7. Get final state
  const state = invokeContract(transferContractId, "get_state");
  console.log("Final state:", state);

  if (state.status !== "Confirmed") {
    throw new Error(`Scenario failed: expected Confirmed, got ${state.status}`);
  }

  console.log("✅ Transfer Lifecycle Scenario Passed!");
}

try {
  runTransferScenario();
} catch (error) {
  console.error("\n❌ Scenario Failed:", error.message);
  process.exit(1);
}
