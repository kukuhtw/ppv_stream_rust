/**
 * scripts/check_balance.js
 * ------------------------------------------------------------
 * Cek saldo wallet admin sebelum deploy smart contract.
 * Jalankan:
 *   npx hardhat run --network megaTestnet scripts/check_balance.js
 *   npx hardhat run --network polygonAmoyTestnet scripts/check_balance.js
 *   npx hardhat run --network polygonMainnet scripts/check_balance.js
 * ------------------------------------------------------------
 */

const hre = require("hardhat");

async function main() {
  const network = await hre.ethers.provider.getNetwork();
  const [signer] = await hre.ethers.getSigners();
  const addr = await signer.getAddress();
  const bal = await hre.ethers.provider.getBalance(addr);

  // Tentukan simbol native token berdasarkan jaringan
  let symbol = "ETH";
  if (String(network.chainId) === "80002" || String(network.name).includes("amoy"))
    symbol = "MATIC";
  else if (String(network.chainId) === "137")
    symbol = "MATIC";
  else if (String(network.chainId) === "6342")
    symbol = "MEGA";

  console.log("==========================================");
  console.log(`🌐 Network : ${network.name} (chainId: ${network.chainId})`);
  console.log(`👤 Signer  : ${addr}`);
  console.log(`💰 Balance : ${hre.ethers.formatEther(bal)} ${symbol}`);
  console.log("==========================================");
}

main().catch((e) => {
  console.error("❌ Check balance failed:", e);
  process.exit(1);
});
