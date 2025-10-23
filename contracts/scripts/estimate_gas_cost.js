console.log("__RUN__", __filename);
/**
 * contracts/scripts/estimate_gas_cost.js
 * ------------------------------------------------------------
 * Estimasi biaya deploy X402Splitter.sol
 * Jalankan:
 *   npx hardhat run --network megaTestnet scripts/estimate_gas_cost.js
 *   npx hardhat run --network polygonMainnet scripts/estimate_gas_cost.js
 *
 * Menampilkan estimasi gas, biaya token, dan estimasi dalam Rupiah.
 * ------------------------------------------------------------
 */

// contracts/scripts/estimate_gas_cost.js
// contracts/scripts/estimate_gas_cost.js
const hre = require("hardhat");

async function main() {
  const admin = process.env.ADMIN_WALLET || process.env.X402_ADMIN_WALLET;
  if (!admin) throw new Error("ADMIN_WALLET / X402_ADMIN_WALLET not set");

  const usdToIdr = Number(process.env.DOLLAR_USD_TO_RUPIAH || "17000");

  const net = await hre.ethers.provider.getNetwork();
  console.log("=====================================================");
  console.log("⛽ Estimating Deploy Cost for X402Splitter.sol");
  console.log("-----------------------------------------------------");
  console.log(`Network: ${hre.network.name}`);
  console.log(`Chain ID: ${net.chainId.toString()}`);
  console.log(`Admin (constructor): ${admin}`);
  console.log("=====================================================");

  // signer & factory
  const [signer] = await hre.ethers.getSigners();
  const Factory = await hre.ethers.getContractFactory("X402Splitter", signer);

  // ethers v6: build deploy transaction request (belum broadcast)
  const txReq = await Factory.getDeployTransaction(admin);

  // Estimasi gas untuk TX deploy
  const gas = await signer.provider.estimateGas(txReq); // BigInt
  console.log(`Estimated gas units: ${gas.toString()}`);

  // Fee data (v6): { gasPrice?, maxFeePerGas?, maxPriorityFeePerGas? } -> BigInt | null
  const fee = await signer.provider.getFeeData();

  // Tentukan "harga gas efektif" yang akan dipakai untuk estimasi biaya
  // Urutan preferensi: maxFeePerGas (EIP-1559) -> gasPrice (legacy)
  const price =
    fee.maxFeePerGas ?? fee.gasPrice ?? null;

  if (!price) {
    console.log("❗ Provider tidak memberi gas price / maxFeePerGas. Tidak bisa estimasi biaya.");
    return;
  }

  const wei = gas * price; // BigInt
  const eth = Number(wei) / 1e18;

  // (Opsional) Jika provider mengembalikan baseFee + priority fee, tampilkan info
  if (fee.maxFeePerGas) {
    console.log(`maxFeePerGas: ${fee.maxFeePerGas.toString()} wei`);
  }
  if (fee.maxPriorityFeePerGas) {
    console.log(`maxPriorityFeePerGas: ${fee.maxPriorityFeePerGas.toString()} wei`);
  }
  if (fee.gasPrice) {
    console.log(`legacy gasPrice: ${fee.gasPrice.toString()} wei`);
  }

  console.log("-----------------------------------------------------");
  console.log(`Effective gas price used: ${price.toString()} wei`);
  console.log(`Estimated cost: ${wei.toString()} wei (~${eth} ETH)`);

  // (Opsional) konversi ke USD/IDR jika kamu punya rate ETH→USD
  // Kalau kamu belum punya oracle harga ETH, kamu bisa skip bagian ini.
  // Di bawah ini, kalau mau: set ENV ETH_USD_PRICE secara manual untuk simulasi
  const ethUsd = Number(process.env.ETH_USD_PRICE || "0");
  if (ethUsd > 0) {
    const costUsd = eth * ethUsd;
    const costIdr = costUsd * usdToIdr;
    console.log(`≈ ${costUsd.toFixed(2)} USD`);
    console.log(`≈ Rp ${Math.round(costIdr).toLocaleString("id-ID")}`);
  } else {
    console.log("(Tip) Set ENV ETH_USD_PRICE untuk lihat konversi USD/IDR.");
  }
  console.log("=====================================================");
}

main().catch((e) => {
  console.error("❌ Error:", e);
  process.exit(1);
});
