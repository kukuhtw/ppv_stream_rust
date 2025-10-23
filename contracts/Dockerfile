
#Dockerfile di folder contracts
FROM node:20-slim
ENV NODE_ENV=development
WORKDIR /app

# Path sumber di-prefix 'contracts/' karena konteks build = root repo
COPY contracts/package.json contracts/package-lock.json* ./
RUN npm install

COPY contracts/hardhat.config.js ./
COPY contracts/contracts ./contracts
COPY contracts/scripts   ./scripts

CMD ["npx", "hardhat", "--help"]
