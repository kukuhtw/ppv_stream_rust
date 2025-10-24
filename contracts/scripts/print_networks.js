// contracts/scripts/print_networks.js

const hre = require("hardhat");

async function main() {
  const keys = Object.keys(hre.config.networks);
  console.log("networks:", keys);
  console.log("defaultNetwork:", hre.config.defaultNetwork);
  if (hre.config.networks.polygonAmoyTestnet) {
    console.log("polygonAmoyTestnet:", hre.config.networks.polygonAmoyTestnet);
  }
}
main().catch((e) => { console.error(e); process.exit(1); });
