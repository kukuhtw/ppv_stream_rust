/**
 * Deploy X402Splitter and write deployment metadata to contracts/deployed.json.
 *
 * Run examples:
 *   npx hardhat run --network polygonAmoyTestnet scripts/deploy_x402.js
 *   npx hardhat run --network polygonMainnet    scripts/deploy_x402.js
 *   npx hardhat run --network megaTestnet       scripts/deploy_x402.js
 *
 * Important environment variables:
 *   ADMIN_WALLET / X402_ADMIN_WALLET   -> admin address passed to the constructor
 *   CONFIRMATIONS=2                    -> optional number of confirmations to wait for
 *   GAS_LIMIT=300000                   -> optional gas limit override
 *   MAX_FEE_GWEI=50                    -> optional EIP-1559 maxFeePerGas override
 *   MAX_PRIORITY_FEE_GWEI=2            -> optional EIP-1559 priority fee override
 *   AUTO_VERIFY=true                   -> optional source-code verification on Polygonscan
 */

const hre = require("hardhat");
const fs = require("fs");
const path = require("path");

function gweiToWeiStr(gweiStr) {
  return hre.ethers.parseUnits(String(gweiStr), "gwei").toString();
}

function getExplorer(chainId) {
  switch (Number(chainId)) {
    case 80002:
      return "https://amoy.polygonscan.com";
    case 137:
      return "https://polygonscan.com";
    case 6342:
      return "https://megaexplorer.io"; // example placeholder
    default:
      return null;
  }
}

function loadJson(p) {
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch (_) {
    return {};
  }
}

function saveJson(p, data) {
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, JSON.stringify(data, null, 2) + "\n");
}

async function main() {
  const admin = process.env.ADMIN_WALLET || process.env.X402_ADMIN_WALLET;
  if (!admin) throw new Error("ADMIN_WALLET / X402_ADMIN_WALLET not set");

  const net = await hre.ethers.provider.getNetwork();
  const networkName = hre.network.name;
  const chainId = Number(net.chainId);
  const explorer = getExplorer(chainId);

  // Optional EIP-1559 transaction overrides.
  const overrides = {};
  if (process.env.GAS_LIMIT) overrides.gasLimit = BigInt(process.env.GAS_LIMIT);
  if (process.env.MAX_FEE_GWEI)
    overrides.maxFeePerGas = BigInt(gweiToWeiStr(process.env.MAX_FEE_GWEI));
  if (process.env.MAX_PRIORITY_FEE_GWEI)
    overrides.maxPriorityFeePerGas = BigInt(
      gweiToWeiStr(process.env.MAX_PRIORITY_FEE_GWEI)
    );

  const confirmations = Number(process.env.CONFIRMATIONS || 1);

  console.log("==============================================");
  console.log("🚀 Deploying X402Splitter...");
  console.log(`🌐 Network   : ${networkName} (chainId: ${chainId})`);
  console.log(`👤 Admin     : ${admin}`);
  if (Object.keys(overrides).length) {
    console.log("⛽ Overrides :", overrides);
  }
  console.log("==============================================");

  const [signer] = await hre.ethers.getSigners();
  const signerAddr = await signer.getAddress();

  const Factory = await hre.ethers.getContractFactory("X402Splitter");
  const contract = await Factory.deploy(admin, overrides);

  const deployTx = contract.deploymentTransaction();
  if (!deployTx) throw new Error("No deployment transaction found");

  console.log(`🧾 Deploy tx : ${deployTx.hash}`);
  console.log("⏳ Waiting for deployment...");
  await contract.waitForDeployment();

  const address = await contract.getAddress(); // ethers v6
  const receipt = await hre.ethers.provider.getTransactionReceipt(deployTx.hash);

  // Optionally wait for extra confirmations before continuing.
  if (confirmations > 1) {
    console.log(`⏳ Waiting for ${confirmations} confirmations...`);
    await hre.ethers.provider.waitForTransaction(deployTx.hash, confirmations);
  }

  console.log("✅ Deployed!");
  console.log(`🏷  Address  : ${address}`);
  console.log(`📦 Block    : ${receipt?.blockNumber ?? "-"}`);
  console.log(`🧑‍💻 Deployer: ${signerAddr}`);
  if (explorer) {
    console.log(`🔗 Explorer : ${explorer}/address/${address}`);
    console.log(`🔗 Tx       : ${explorer}/tx/${deployTx.hash}`);
  }

  // =========================================================
  // Write or merge deployment metadata into contracts/deployed.json.
  // =========================================================
  const outPath = path.join(__dirname, "..", "deployed.json");
  const existing = loadJson(outPath);

  const nowIso = new Date().toISOString();

  // Store metadata per chain ID and contract name.
  existing[chainId] = existing[chainId] || {};
  existing[chainId]["X402Splitter"] = {
    address,
    admin,
    networkName,
    chainId,
    deployTx: deployTx.hash,
    blockNumber: receipt?.blockNumber ?? null,
    deployedAt: nowIso,
    explorer,
    constructorArgs: [admin],
    implementationNote:
      "Simple splitter for native/ERC20. Update this note if you upgrade contract.",
  };

  // Also store the latest active deployment by Hardhat network name.
  existing["_latest"] = existing["_latest"] || {};
  existing["_latest"][networkName] = {
    address,
    chainId,
    contract: "X402Splitter",
    updatedAt: nowIso,
  };

  saveJson(outPath, existing);
  console.log(`📝 Written to: ${path.relative(process.cwd(), outPath)}`);

  // =========================================================
  // Optional source-code verification on Polygonscan.
  // =========================================================
  if (process.env.AUTO_VERIFY === "true") {
    try {
      console.log("🛠  Verifying on explorer...");
      await hre.run("verify:verify", {
        address,
        constructorArguments: [admin],
      });
      console.log("✅ Verify success");
    } catch (err) {
      console.warn("⚠️  Verify skipped/failed:", err?.message || err);
    }
  }
}

main().catch((e) => {
  console.error("❌ Deployment failed:", e);
  process.exit(1);
});