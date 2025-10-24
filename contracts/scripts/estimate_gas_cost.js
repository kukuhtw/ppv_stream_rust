// contracts/scripts/estimate_gas_cost.js
console.log("__RUN__", __filename);
/**
 * Estimasi biaya deploy X402Splitter.sol (multi-network, EIP-1559 aware)
 * Jalankan contoh:
 *   npx hardhat run --network polygonAmoyTestnet scripts/estimate_gas_cost.js
 *   npx hardhat run --network polygonMainnet    scripts/estimate_gas_cost.js
 *   npx hardhat run --network megaTestnet       scripts/estimate_gas_cost.js
 *
 * ENV opsional:
 *   ADMIN_WALLET / X402_ADMIN_WALLET  -> constructor arg
 *   DOLLAR_USD_TO_RUPIAH=17000
 *   # override fee
 *   GAS_LIMIT=300000
 *   MAX_FEE_GWEI=50
 *   MAX_PRIORITY_FEE_GWEI=2
 *   # harga koin (pilih yang relevan)
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

// Tentukan simbol native & env harga USD yg dipakai
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

  // Factory & tx request (belum broadcast)
  const Factory = await hre.ethers.getContractFactory("X402Splitter", signer);
  const txReq = await Factory.getDeployTransaction(admin);

  // pastikan from di-set agar estimateGas akurat
  txReq.from = signerAddr;

  // Overrides via ENV (opsional)
  if (process.env.GAS_LIMIT) txReq.gasLimit = BigInt(process.env.GAS_LIMIT);
  const feeData = await signer.provider.getFeeData();
  const latestBlock = await signer.provider.getBlock("latest");

  // Pilih fee yang dipakai untuk "upper bound"
  // prefer MAX_FEE_GWEI env -> feeData.maxFeePerGas -> feeData.gasPrice
  let maxFeePerGas =
    process.env.MAX_FEE_GWEI
      ? gweiToWeiBig(process.env.MAX_FEE_GWEI)
      : feeData.maxFeePerGas ?? feeData.gasPrice;

  // Priority yang dipakai (untuk lower bound)
  let priorityFee =
    process.env.MAX_PRIORITY_FEE_GWEI
      ? gweiToWeiBig(process.env.MAX_PRIORITY_FEE_GWEI)
      : feeData.maxPriorityFeePerGas ?? gweiToWeiBig(1); // fallback 1 gwei

  if (!maxFeePerGas) {
    console.log("❗ Provider tidak memberi gas price / maxFeePerGas. Tidak bisa estimasi biaya.");
    return;
  }

  // Estimasi gas units
  const gasUnits = txReq.gasLimit
    ? BigInt(txReq.gasLimit)
    : await signer.provider.estimateGas(txReq);

  // Upper bound = gas * maxFeePerGas
  const upperWei = gasUnits * maxFeePerGas;

  // Lower bound (jika ada baseFee) = gas * (baseFee + priority)
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
    console.log(`(Tip) Set ENV ${priceEnv} untuk konversi ke USD/IDR. Contoh: ${priceEnv}=0.62`);
  }
  console.log("=====================================================");
}

main().catch((e) => {
  console.error("❌ Error:", e);
  process.exit(1);
});
