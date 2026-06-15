// contracts/hardhat.config.js
//
// Hardhat configuration for compiling, testing, deploying, and verifying the
// X402Splitter smart contract.
//
// This file is responsible for:
// 1. Loading Hardhat plugins and environment variables.
// 2. Defining the Solidity compiler version and optimizer settings.
// 3. Configuring Polygon Amoy testnet, optional Polygon mainnet, and local Hardhat.
// 4. Supplying the deployment wallet private key to configured networks.
// 5. Configuring Polygonscan source-code verification.
//
// Security note:
// Never hardcode private keys, RPC credentials, or explorer API keys here.
// Store secrets in a local `.env` file or a secure secret-management system.

// Hardhat Toolbox provides common development plugins, including Ethers,
// Chai matchers, contract verification support, and Solidity testing utilities.
require("@nomicfoundation/hardhat-toolbox");

// Load environment variables from `contracts/.env` into `process.env`.
require("dotenv").config();

// Read deployment and network configuration from the environment.
const {
  // Private key of the wallet that signs and pays for deployment transactions.
  // The expected value is a hexadecimal private key, normally prefixed with 0x.
  PRIVATE_KEY,

  // Polygon Amoy testnet RPC endpoint and chain ID.
  // Chain ID 80002 is the standard Polygon Amoy testnet identifier.
  AMOY_RPC_HTTP,
  AMOY_CHAIN_ID,

  // Polygon mainnet RPC endpoint and chain ID.
  // Chain ID 137 is the standard Polygon PoS mainnet identifier.
  POLYGON_RPC_HTTP,
  POLYGON_CHAIN_ID,

  // Polygonscan API key used by Hardhat's verification task.
  POLYGONSCAN_API_KEY
} = process.env;

module.exports = {
  // -------------------------------------------------------------------------
  // Solidity compiler configuration
  // -------------------------------------------------------------------------

  solidity: {
    // The compiler version must match the contract pragma and must remain
    // identical when verifying the deployed contract on Polygonscan.
    version: "0.8.20",

    settings: {
      optimizer: {
        // Enable bytecode optimization to reduce runtime gas usage and contract
        // size. Changing this value after deployment can break verification.
        enabled: true,

        // `runs` describes the expected number of contract executions.
        // A value of 200 is a common balance between deployment size and
        // long-term runtime gas efficiency.
        runs: 200
      }
    }
  },

  // Use Polygon Amoy when a Hardhat command does not provide `--network`.
  // Production scripts should still pass the network explicitly to reduce the
  // risk of deploying to the wrong chain.
  defaultNetwork: "polygonAmoyTestnet",

  // -------------------------------------------------------------------------
  // Network configuration
  // -------------------------------------------------------------------------

  networks: {
    // Polygon Amoy testnet is the default environment for development,
    // integration testing, gas estimation, and deployment rehearsal.
    polygonAmoyTestnet: {
      // Prefer the configured private RPC endpoint. The public endpoint is only
      // a fallback and may be rate limited or unavailable.
      url: AMOY_RPC_HTTP || "https://polygon-amoy-bor.publicnode.com",

      // Convert the environment value from string to number. Use the official
      // Amoy chain ID when the variable is not provided.
      chainId: Number(AMOY_CHAIN_ID || 80002),

      // Hardhat creates a signer from PRIVATE_KEY. An empty array allows compile
      // and read-only operations but deployment will fail because no signer exists.
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : []
    },

    // Polygon mainnet is intentionally disabled by default to reduce accidental
    // production deployment risk. Enable this block only after testnet validation,
    // security review, wallet verification, and gas-cost approval are complete.
    /*
    polygonMainnet: {
      url: POLYGON_RPC_HTTP || "https://polygon-rpc.com",
      chainId: Number(POLYGON_CHAIN_ID || 137),
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : []
    },
    */

    // Local in-memory Hardhat network used for unit tests and local development.
    // Accounts are generated automatically and must never be used on public chains.
    hardhat: {
      chainId: 31337
    }
  },

  // -------------------------------------------------------------------------
  // Polygonscan contract verification configuration
  // -------------------------------------------------------------------------

  etherscan: {
    // Hardhat selects the key by network name. Multiple aliases are provided so
    // verification remains compatible with both standard and project-specific
    // Amoy network names.
    apiKey: {
      polygon: POLYGONSCAN_API_KEY || "",
      polygonAmoy: POLYGONSCAN_API_KEY || "",
      polygonAmoyTestnet: POLYGONSCAN_API_KEY || ""
    },

    // Define custom explorer endpoints for network aliases not known directly
    // by the installed Hardhat verification plugin version.
    customChains: [
      {
        // This name must exactly match the key under `networks` and the value
        // supplied through `--network polygonAmoyTestnet`.
        network: "polygonAmoyTestnet",
        chainId: 80002,
        urls: {
          // REST API endpoint used by Hardhat to submit verification metadata.
          apiURL: "https://api-amoy.polygonscan.com/api",

          // Human-facing explorer URL used to inspect contracts and transactions.
          browserURL: "https://amoy.polygonscan.com"
        }
      },
      {
        // Optional alias for future commands using `--network polygonAmoy`.
        // A matching `networks.polygonAmoy` entry must also exist before the
        // alias can be used for deployment or other network operations.
        network: "polygonAmoy",
        chainId: 80002,
        urls: {
          apiURL: "https://api-amoy.polygonscan.com/api",
          browserURL: "https://amoy.polygonscan.com"
        }
      }
    ]
  }
};
