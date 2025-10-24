// contracts/hardhat.config.js
require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

const {
  PRIVATE_KEY,

  // ===== Polygon Amoy Testnet =====
  AMOY_RPC_HTTP,
  AMOY_CHAIN_ID,

  // ===== Polygon Mainnet =====
  POLYGON_RPC_HTTP,
  POLYGON_CHAIN_ID,

  // (Opsional) Polygonscan API Key (untuk verifikasi kontrak)
  POLYGONSCAN_API_KEY
} = process.env;

module.exports = {
  solidity: {
    version: "0.8.20",
    settings: { optimizer: { enabled: true, runs: 200 } }
  },

  // Default gunakan Amoy Testnet
  defaultNetwork: "polygonAmoyTestnet",

  networks: {
    // ✅ Polygon Amoy Testnet
    polygonAmoyTestnet: {
      url: AMOY_RPC_HTTP || "https://polygon-amoy-bor.publicnode.com",
      chainId: Number(AMOY_CHAIN_ID || 80002),
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : []
    },

    // 💎 Polygon Mainnet (aktifkan kalau perlu)
    /*
    polygonMainnet: {
      url: POLYGON_RPC_HTTP || "https://polygon-rpc.com",
      chainId: Number(POLYGON_CHAIN_ID || 137),
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : []
    },
    */

    // Local
    hardhat: { chainId: 31337 }
  },

  // ============================
  // Polygonscan Verification
  // ============================
  // KUNCINYA: samakan nama network dengan `--network polygonAmoyTestnet`
  etherscan: {
    apiKey: {
      // Sediakan keduanya supaya fleksibel
      polygon: POLYGONSCAN_API_KEY || "",
      polygonAmoy: POLYGONSCAN_API_KEY || "",
      polygonAmoyTestnet: POLYGONSCAN_API_KEY || ""
    },
    customChains: [
      // Alias: bila kamu jalankan `--network polygonAmoyTestnet`
      {
        network: "polygonAmoyTestnet",
        chainId: 80002,
        urls: {
          apiURL: "https://api-amoy.polygonscan.com/api",
          browserURL: "https://amoy.polygonscan.com"
        }
      },
      // Alias tambahan (kalau suatu saat pakai nama "polygonAmoy")
      {
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
