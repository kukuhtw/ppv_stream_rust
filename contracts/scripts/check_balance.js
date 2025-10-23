/**
 * scripts/check_balance.js
 * ------------------------------------------------------------
 * Cek saldo wallet admin sebelum deploy smart contract.
 * Jalankan:
 *   npx hardhat run --network megaTestnet scripts/check_balance.js
 *   npx hardhat run --network polygonMainnet scripts/check_balance.js
 * ------------------------------------------------------------
 */

// contracts/scripts/check_balance.js
const hre = require("hardhat");

async function main() {
  const [signer] = await hre.ethers.getSigners();
  const addr = await signer.getAddress();
  const bal = await hre.ethers.provider.getBalance(addr);
  console.log(`Signer: ${addr}`);
  console.log(`Balance: ${hre.ethers.formatEther(bal)} ETH`);
}

main()
  .then(() => process.exit(0))
  .catch((e) => {
    console.error("❌ Check balance failed:", e);
    process.exit(1);
  });
