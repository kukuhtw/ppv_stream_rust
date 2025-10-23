/**
 * contracts/scripts/deploy_x402.js
 * Deploy script for X402Splitter
 * ---------------------------------------------
 * 1️⃣ Deploy kontrak ke network aktif
 * 2️⃣ Tulis hasil ke contracts/deployed.json
 */

// contracts/scripts/deploy_x402.js
const hre = require("hardhat");

async function main() {
  const admin =
    process.env.ADMIN_WALLET || process.env.X402_ADMIN_WALLET;
  if (!admin) throw new Error("ADMIN_WALLET / X402_ADMIN_WALLET not set");

  const net = await hre.ethers.provider.getNetwork();

  console.log("==============================================");
  console.log("🚀 Deploying X402Splitter...");
  console.log(`Network: ${hre.network.name}`);
  console.log(`Chain ID: ${net.chainId.toString()}`);
  console.log(`Admin Wallet: ${admin}`);
  console.log("==============================================");

  const Factory = await hre.ethers.getContractFactory("X402Splitter");
  const contract = await Factory.deploy(admin);

  // ⬇️ ethers v6
  await contract.waitForDeployment();
  const addr = await contract.getAddress(); // atau contract.target

  console.log(`X402Splitter deployed at: ${addr}`);
}

main()
  .then(() => process.exit(0))
  .catch((e) => {
    console.error("❌ Deployment failed:", e);
    process.exit(1);
  });
