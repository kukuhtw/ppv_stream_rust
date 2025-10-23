require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

const {
  PRIVATE_KEY,
  // Testnet (Mega)
  MEGA_RPC_HTTP,
  MEGA_CHAIN_ID,
  // Mainnet (Polygon)
  POLYGON_RPC_HTTP,
  POLYGON_CHAIN_ID
} = process.env;

module.exports = {
  solidity: "0.8.20",
  networks: {
    megaTestnet: {
      url: MEGA_RPC_HTTP || "https://carrot.megaeth.com/rpc",
      chainId: Number(MEGA_CHAIN_ID || 6342),
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : []
    },
    polygonMainnet: {
      url: POLYGON_RPC_HTTP || "https://polygon-rpc.com",
      chainId: Number(POLYGON_CHAIN_ID || 137),
      accounts: PRIVATE_KEY ? [PRIVATE_KEY] : []
    }
  }
};
