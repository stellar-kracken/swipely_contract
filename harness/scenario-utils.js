const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const contractsDir = path.resolve(__dirname, "../contracts/soroban");
const wasmDir = path.resolve(contractsDir, "target/wasm32-unknown-unknown/release");

function run(command, options = {}) {
  return execSync(command, {
    stdio: "pipe",
    encoding: "utf-8",
    ...options,
  });
}

function buildContracts() {
  console.log("Building contracts...");
  run("cargo build --release", { cwd: contractsDir });
}

function getWasmPath(contractName) {
  const wasmFile = `${contractName}.wasm`;
  const wasmPath = path.join(wasmDir, wasmFile);
  if (!fs.existsSync(wasmPath)) {
    throw new Error(`WASM file not found for ${contractName} at ${wasmPath}. Did you build the contracts?`);
  }
  return wasmPath;
}

function deployContract(contractName) {
  const wasmPath = getWasmPath(contractName);
  console.log(`Deploying ${contractName}...`);
  const output = run(`soroban contract deploy --wasm ${wasmPath}`);
  const contractId = output.trim();
  console.log(`Deployed ${contractName} with ID: ${contractId}`);
  return contractId;
}

function invokeContract(contractId, method, args = {}) {
  const argsString = Object.entries(args)
    .map(([key, value]) => `--${key} '${JSON.stringify(value)}'`)
    .join(" ");

  console.log(`Invoking ${method} on ${contractId}...`);
  const output = run(`soroban contract invoke --id ${contractId} --fn ${method} ${argsString}`);
  return JSON.parse(output);
}

function resetState() {
  console.log("Resetting local network state...");
  run("soroban network reset");
}

module.exports = {
  buildContracts,
  deployContract,
  invokeContract,
  resetState,
  getWasmPath,
};
