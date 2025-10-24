# contracts/Dockerfile
# Image kecil, tapi lengkap tools buat Hardhat + Makefile hooks
FROM node:20-slim

# 1) Sistem tools yang sering dibutuhin hardhat/plugins & Makefile
#    - bash           : biar script heredoc di Makefile aman
#    - jq             : Makefile ambil address dari deployed.json
#    - curl, ca-certs : sanity check RPC / health
#    - git, openssl   : beberapa plugin/verify butuh ini
#    - python3, make, g++ : build native deps (keccak, secp256k1, dsb)
RUN apt-get update && apt-get install -y --no-install-recommends \
      bash jq curl ca-certificates git openssl python3 make g++ \
    && rm -rf /var/lib/apt/lists/*

# (Opsional) init process supaya reaping child process rapi
#RUN apt-get update && apt-get install -y --no-install-recommends tini \
#    && rm -rf /var/lib/apt/lists/*
#ENTRYPOINT ["/usr/bin/tini","--"]

ENV NODE_ENV=development
WORKDIR /app

# 2) Copy hanya file dependency dulu untuk cache layer npm
#    (Konteks build = root repo → prefix "contracts/")
COPY contracts/package.json contracts/package-lock.json* ./

# Gunakan npm ci agar deterministic; kalau ga ada lock, fallback ke install
RUN if [ -f package-lock.json ]; then npm ci; else npm install; fi

# 3) Copy sisa source Hardhat
COPY contracts/hardhat.config.js ./hardhat.config.js
COPY contracts/contracts ./contracts
COPY contracts/scripts   ./scripts

# (Opsional) kalau kamu pakai artifacts/cache dari host, uncomment ini:
# COPY contracts/cache ./cache
# COPY contracts/artifacts ./artifacts

# 4) Default command (Makefile akan override via `npx hardhat run ...`)
CMD ["npx", "hardhat", "--help"]
