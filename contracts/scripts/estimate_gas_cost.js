// contracts/scripts/estimate_gas_cost.js
console.log("__RUN__", __filename);
/**
 * Estimate the deployment cost for X402Splitter.sol.
 *
 * This script is multi-network aware and supports EIP-1559 fee fields when the
 * selected network provides them.
 *
 * Example commands:
 *   npx hardhat run --network polygonAmoyTestnet scripts/estimate_gas_cost.js
 *   npx hardhat run --network polygonMainnet    scripts/estimate_gas_cost.js
 *   npx hardhat run --network megaTestnet       scripts/estimate_gas_cost.js
 *
 * Optional environment variables:
 *   ADMIN_WALLET / X402_ADMIN_WALLET  -> constructor argument
 *   DOLLAR_USD_TO_RUPIAH=17000
 *
 * Fee overrides:
 *   GAS_LIMIT=300000
 *   MAX_FEE_GWEI=50
 *   MAX_PRIORITY_FEE_GWEI=2
 *
 * Native coin USD price. Set the variable that matches the selected network:
 *   ETH_USD_PRICE=2450
 *   MATIC_USD_PRICE=0.62
 *   MEGA_USD_PRICE=0.01
 */

const hre = require("hardhat");

function gweiToWeiBig(g) {
  return hre.ethers.parseUnits(String(g), "gwei");
}

function fmtGwei(bn) {
  return hre.ethers.formatUnits(bn, "gwei");
}
function fmtEther(bn) {
  return hre.ethers.formatEther(bn);
}

// Determine the native coin symbol and the USD price environment variable.
function nativeMeta(chainId) {
  switch (Number(chainId)) {
    case 80002:
    case 137:
      return { symbol: "MATIC", priceEnv: "MATIC_USD_PRICE" };
    case 6342:
      return { symbol: "MEGA", priceEnv: "MEGA_USD_PRICE" };
    default:
      return { symbol: "ETH", priceEnv: "ETH_USD_PRICE" };
  }
}

async function main() {
  const admin = process.env.ADMIN_WALLET || process.env.X402_ADMIN_WALLET;
  if (!admin) throw new Error("ADMIN_WALLET / X402_ADMIN_WALLET not set");

  const usdToIdr = Number(process.env.DOLLAR_USD_TO_RUPIAH || "17000");

  const net = await hre.ethers.provider.getNetwork();
  const { symbol, priceEnv } = nativeMeta(net.chainId);
  const coinUsd = Number(process.env[priceEnv] || "0");

  console.log("=====================================================");
  console.log("⛽ Estimating Deploy Cost for X402Splitter.sol");
  console.log("-----------------------------------------------------");
  console.log(`Network        : ${hre.network.name}`);
  console.log(`Chain ID       : ${net.chainId.toString()}`);
  console.log(`Admin (ctor)   : ${admin}`);
  console.log("=====================================================");

  const [signer] = await hre.ethers.getSigners();
  const signerAddr = await signer.getAddress();

  // Build the contract factory and deployment transaction request without broadcasting.
  const Factory = await hre.ethers.getContractFactory("X402Splitter", signer);
  const txReq = await Factory.getDeployTransaction(admin);

  // Set the sender explicitly so gas estimation is more accurate.
  txReq.from = signerAddr;

  // Optional environment-driven overrides.
  if (process.env.GAS_LIMIT) txReq.gasLimit = BigInt(process.env.GAS_LIMIT);
  const feeData = await signer.provider.getFeeData();
  const latestBlock = await signer.provider.getBlock("latest");

  // Select the fee value used for the upper-bound estimate.
  // Priority: MAX_FEE_GWEI env -> provider maxFeePerGas -> provider gasPrice.
  let maxFeePerGas =
    process.env.MAX_FEE_GWEI
      ? gweiToWeiBig(process.env.MAX_FEE_GWEI)
      : feeData.maxFeePerGas ?? feeData.gasPrice;

  // Select the priority fee used for the lower-bound estimate.
  let priorityFee =
    process.env.MAX_PRIORITY_FEE_GWEI
      ? gweiToWeiBig(process.env.MAX_PRIORITY_FEE_GWEI)
      : feeData.maxPriorityFeePerGas ?? gweiToWeiBig(1); // fallback 1 gwei

  if (!maxFeePerGas) {
    console.log("❗ Provider did not return gasPrice or maxFeePerGas. Deployment cost cannot be estimated.");
    return;
  }

  // Estimate gas units.
  const gasUnits = txReq.gasLimit
    ? BigInt(txReq.gasLimit)
    : await signer.provider.estimateGas(txReq);

  // Upper bound = gas units * maxFeePerGas.
  const upperWei = gasUnits * maxFeePerGas;

  // Lower bound, when baseFee is available, = gas units * (baseFee + priorityFee).
  let lowerWei = upperWei;
  if (latestBlock && latestBlock.baseFeePerGas != null) {
    const eff = latestBlock.baseFeePerGas + priorityFee;
    lowerWei = gasUnits * eff;
  }

  console.log("-----------------------------------------------------");
  console.log(`Estimated gas units       : ${gasUnits.toString()}`);
  if (latestBlock && latestBlock.baseFeePerGas != null) {
    console.log(`baseFeePerGas (wei)       : ${latestBlock.baseFeePerGas.toString()} (${fmtGwei(latestBlock.baseFeePerGas)} gwei)`);
  }
  if (feeData.maxPriorityFeePerGas) {
    console.log(`suggested priority (wei)  : ${feeData.maxPriorityFeePerGas.toString()} (${fmtGwei(feeData.maxPriorityFeePerGas)} gwei)`);
  }
  if (feeData.maxFeePerGas) {
    console.log(`suggested maxFee (wei)    : ${feeData.maxFeePerGas.toString()} (${fmtGwei(feeData.maxFeePerGas)} gwei)`);
  }
  if (feeData.gasPrice) {
    console.log(`legacy gasPrice (wei)     : ${feeData.gasPrice.toString()} (${fmtGwei(feeData.gasPrice)} gwei)`);
  }
  if (process.env.MAX_FEE_GWEI || process.env.MAX_PRIORITY_FEE_GWEI) {
    console.log(`OVERRIDE maxFee/priority  : ${process.env.MAX_FEE_GWEI || "-"} / ${process.env.MAX_PRIORITY_FEE_GWEI || "-"} gwei`);
  }
  console.log("-----------------------------------------------------");
  console.log(`Upper bound cost          : ${upperWei.toString()} wei (~${fmtEther(upperWei)} ${symbol})`);
  if (lowerWei !== upperWei) {
    console.log(`Lower bound cost          : ${lowerWei.toString()} wei (~${fmtEther(lowerWei)} ${symbol})`);
  }

  if (coinUsd > 0) {
    const upperUsd = Number(fmtEther(upperWei)) * coinUsd;
    const lowerUsd = Number(fmtEther(lowerWei)) * coinUsd;
    const upperIdr = Math.round(upperUsd * usdToIdr);
    const lowerIdr = Math.round(lowerUsd * usdToIdr);

    console.log("-----------------------------------------------------");
    if (lowerWei !== upperWei) {
      console.log(`≈ Lower:  $${lowerUsd.toFixed(2)}  | Rp ${lowerIdr.toLocaleString("id-ID")}`);
    }
    console.log(`≈ Upper:  $${upperUsd.toFixed(2)}  | Rp ${upperIdr.toLocaleString("id-ID")}`);
  } else {
    console.log(`(Tip) Set ${priceEnv} to enable USD and IDR conversion. Example: ${priceEnv}=0.62`);
  }
  console.log("=====================================================");
}

main().catch((e) => {
  console.error("❌ Error:", e);
  process.exit(1);
});