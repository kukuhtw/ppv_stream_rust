# ===========================
# Global config
# ===========================
SHELL          := /bin/bash
APP_NAME       ?= ppv_stream
IMAGE_TAG      ?= $(APP_NAME):dev
BUILD_REV      ?= $(shell date +%s)
COMPOSE        ?= docker compose
NET            ?= polygonAmoyTestnet   # polygonAmoyTestnet (default) atau polygonMainnet

# Cross-platform sed -i (GNU vs macOS/BSD)
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
  SED_INPLACE := sed -i ''
else
  SED_INPLACE := sed -i
endif

.PHONY: help db-up db-down db-reset db-shell db-psql migrate seed build rebuild run run-all logs logs-db logs-all stop down sh adminer-up adminer-open ps health adminer-check clean wait-db \
        deployx402 estimatex402 showx402 deployx402-mainnet checkx402

# ===========================
# HELP
# ===========================
help:
	@echo ""
	@echo "QUICK DEPLOY - fastest way to run the full stack"
	@echo "====================================================="
	@echo "  0) Create .env from .env.example and fill the required variables:"
	@echo "     - DATABASE_URL, DATABASE_URL_BUILD, HMAC_SECRET, DOLLAR_USD_TO_RUPIAH"
	@echo "     - Payment credentials you want to use: PAYPAL_*, XENDIT_*, STRIPE_*, MIDTRANS_*"
	@echo "     - X402_CONTRACT_ADDRESS and chain variables if x402 is enabled"
	@echo "     - Optional ADMIN_BOOTSTRAP_* values to create the first admin"
	@echo ""
	@echo "  1) make db-up        -> start PostgreSQL"
	@echo "  2) make migrate      -> apply ./sql and ./migrations"
	@echo "  3) make build        -> build the app image"
	@echo "  4) make run-all      -> start the app after DB is healthy"
	@echo "  5) make seed         -> load 10 demo users for login testing"
	@echo "  6) Open http://localhost:8080 and Adminer at http://localhost:8081"
	@echo ""
	@echo "  After the first admin login:"
	@echo "  - Go to Admin > Settings > Payment Methods"
	@echo "  - Enable PayPal, Stripe, Xendit, Midtrans, Wallet, Wallet Transfer, or x402"
	@echo "  - Choose the default fiat payment provider"
	@echo ""
	@echo "====================================================="
	@echo "SMART CONTRACT - x402 Splitter (deploy once per network)"
	@echo "====================================================="
	@echo "  Required in .env: PRIVATE_KEY, X402_ADMIN_WALLET,"
	@echo "  plus AMOY_RPC_HTTP / AMOY_CHAIN_ID for polygonAmoyTestnet or Polygon mainnet values."
	@echo ""
	@echo "  Check admin wallet balance:"
	@echo "    make checkx402 [NET=polygonAmoyTestnet|polygonMainnet]"
	@echo "  Estimate deployment gas:"
	@echo "    make estimatex402 [NET=polygonAmoyTestnet|polygonMainnet]"
	@echo "  Deploy testnet (Polygon Amoy default):"
	@echo "    make deployx402 [NET=polygonAmoyTestnet]"
	@echo "  Deploy Polygon mainnet:"
	@echo "    make deployx402-mainnet"
	@echo "  Show the contract address saved in .env:"
	@echo "    make showx402"
	@echo ""
	@echo "  Note: deploy the smart contract once per network, then store it in .env:"
	@echo "        X402_CONTRACT_ADDRESS=0x.... and the app will use that address."
	@echo ""
	@echo "====================================================="
	@echo "DATABASE MANAGEMENT"
	@echo "====================================================="
	@echo "  db-up           : Start the Postgres container."
	@echo "  db-down         : Stop the database container without deleting data."
	@echo "  db-reset        : Remove pgdata volume, start DB, then run migrations."
	@echo "  db-shell        : Open a shell inside the database container."
	@echo "  db-psql         : Open psql inside the database container."
	@echo "  wait-db         : Wait until the database is healthy."
	@echo ""
	@echo "====================================================="
	@echo "DATABASE MIGRATION & SEED"
	@echo "====================================================="
	@echo "  migrate         : Apply all SQL files in ./sql and then ./migrations in sorted order."
	@echo "  seed            : Run the seed_dummy binary in the app container."
	@echo "                    Example login: user03@example.com / Passw0rd03!"
	@echo ""
	@echo "====================================================="
	@echo "BUILD & RUNTIME"
	@echo "====================================================="
	@echo "  build           : Build the app image with cache."
	@echo "  rebuild         : Rebuild without cache, then start the app."
	@echo "  run             : Start all docker-compose services."
	@echo "  run-all         : Start DB, wait for health, then start the app."
	@echo ""
	@echo "====================================================="
	@echo "LOGS & MONITORING"
	@echo "====================================================="
	@echo "  logs            : Stream app container logs."
	@echo "  logs-db         : Stream database logs."
	@echo "  logs-all        : Stream logs from all services."
	@echo "  ps              : Show container status."
	@echo "  health          : Check the /health endpoint."
	@echo ""
	@echo "====================================================="
	@echo "ADMIN & MAINTENANCE"
	@echo "====================================================="
	@echo "  sh              : Open a shell in the app container."
	@echo "  stop            : Stop all containers without removing them."
	@echo "  down            : Stop all containers and remove the network."
	@echo "  clean           : Remove the local Docker app image ($(IMAGE_TAG))."
	@echo ""
	@echo "====================================================="
	@echo "ADMINER (Database Web UI)"
	@echo "====================================================="
	@echo "  adminer-up      : Start Adminer on port 8081."
	@echo "  adminer-open    : Open Adminer in the browser (http://localhost:8081)."
	@echo "  adminer-check   : Run an HTTP HEAD check against Adminer."
	@echo ""

# ===========================
# Database
# ===========================
db-up:
	$(COMPOSE) up -d db

db-down:
	$(COMPOSE) stop db

db-reset:
	@echo "!!! WARNING: Menghapus volume pgdata !!!"
	$(COMPOSE) down
	-@V=$$(docker volume ls -q | grep -E 'pgdata$$' || true); \
	if [ -n "$$V" ]; then docker volume rm $$V; else echo "(no pgdata volume found)"; fi
	$(COMPOSE) up -d db
	$(MAKE) wait-db
	$(MAKE) migrate

db-shell:
	docker exec -it ppv_stream_db bash

db-psql:
	docker exec -it ppv_stream_db psql -U ppv -d ppv_stream

wait-db:
	@echo "==> waiting for db health..."
	@for i in $$(seq 1 30); do \
	  S=$$(docker inspect -f '{{.State.Health.Status}}' ppv_stream_db 2>/dev/null || echo "unknown"); \
	  echo "db health: $$S"; \
	  if [ "$$S" = "healthy" ]; then exit 0; fi; \
	  sleep 1; \
	done; \
	echo "DB not healthy in time"; exit 1

# ===========================
# Migrations & Seed
# ===========================
migrate:
	@echo "==> Apply SQL schema dari ./sql ..."
	@if [ -d sql ] && ls -1 sql/*.sql >/dev/null 2>&1; then \
	  for f in $$(find sql -maxdepth 1 -type f -name '*.sql' | sort -V); do \
	    echo "-> $$f"; \
	    docker exec -i ppv_stream_db psql -v ON_ERROR_STOP=1 -U ppv -d ppv_stream -f - < $$f || exit 1; \
	  done; \
	else \
	  echo "(skip: tidak ada file di ./sql)"; \
	fi
	@echo "==> Apply incremental migrations dari ./migrations ..."
	@if [ -d migrations ] && ls -1 migrations/*.sql >/dev/null 2>&1; then \
	  for f in $$(find migrations -maxdepth 1 -type f -name '*.sql' | sort -V); do \
	    echo "-> $$f"; \
	    docker exec -i ppv_stream_db psql -v ON_ERROR_STOP=1 -U ppv -d ppv_stream -f - < $$f || exit 1; \
	  done; \
	else \
	  echo "(skip: tidak ada file di ./migrations)"; \
	fi
	@echo "All migrations completed successfully."

seed:
	@echo "==> Seeding 10 dummy users via binary..."
	@docker exec \
	  -e RUST_LOG=info \
	  -e DATABASE_URL=postgres://ppv:secret@db:5432/ppv_stream \
	  ppv_stream_app /usr/local/bin/seed_dummy || (echo "seed failed"; exit 1)

# ===========================
# App lifecycle
# ===========================
build:
	$(COMPOSE) build app --build-arg BUILD_REV=$(BUILD_REV)

rebuild:
	$(COMPOSE) build app --no-cache --build-arg BUILD_REV=$(BUILD_REV)
	$(COMPOSE) up -d app
	$(MAKE) logs

run:
	$(COMPOSE) up -d

run-all: db-up build
	$(MAKE) wait-db
	$(COMPOSE) up -d app

logs:
	$(COMPOSE) logs -f app

logs-db:
	$(COMPOSE) logs -f db

logs-all:
	$(COMPOSE) logs -f

stop:
	$(COMPOSE) stop

down:
	$(COMPOSE) down

sh:
	-@docker exec -it ppv_stream_app bash || (echo "container belum jalan? jalankan 'make run' dulu")

# ===========================
# Adminer
# ===========================
adminer-up:
	$(COMPOSE) up -d adminer

adminer-open:
	@URL="http://localhost:8081"; \
	if command -v wslview >/dev/null 2>&1; then wslview $$URL >/dev/null 2>&1 || true; \
	elif command -v xdg-open >/dev/null 2>&1; then xdg-open $$URL >/dev/null 2>&1 || true; \
	elif command -v open >/dev/null 2>&1; then open $$URL >/dev/null 2>&1 || true; \
	else echo "Open this URL in your browser: $$URL"; fi

ps:
	$(COMPOSE) ps

health:
	@RC=0; OUT=$$(curl -fsS -w " [HTTP:%{http_code}]\n" http://localhost:8080/health || RC=$$?; echo $$OUT); exit $$RC

adminer-check:
	@curl -fsSI http://localhost:8081 >/dev/null || (echo "Adminer belum up?" && exit 1)

clean:
	-@docker rmi $(IMAGE_TAG) 2>/dev/null || true

rebuild-deployer:
	$(COMPOSE) build x402-deployer --no-cache

show-deployer-networks:
	@$(COMPOSE) $(DEPLOY_ENV_FILES) run --rm x402-deployer \
	  npx hardhat run scripts/print_networks.js

# ===========================
# X402 DEPLOYER (Hardhat)
# ===========================
# NOTE:
# - We DO NOT re-declare COMPOSE here (use the one at the top)
# - Read variables from .env and (if exists) contracts/.env

# Helper: set/update key=val in .env (without sed -i macOS/GNU issues)
define _env_set
	@awk -v key="$(1)" -v val="$(2)" 'BEGIN{found=0} \
	  /^[[:space:]]*#/ {print; next} \
	  $$0 ~ "^"key"=" {print key"="val; found=1; next} \
	  {print} \
	  END{if(!found) print key"="val}' .env > .env.tmp && mv .env.tmp .env
endef

# Build env-file flags dynamically: always root .env; add contracts/.env if present
DEPLOY_ENV_FILES := --env-file .env
ifneq ("$(wildcard contracts/.env)","")
  DEPLOY_ENV_FILES += --env-file contracts/.env
endif

# ===========================
# ENV CHECK (inline bash)
# ===========================
define _check_env_base
	@bash -c ' \
	set -euo pipefail; \
	NET="$(NET)"; \
	files=(".env"); \
	[ -f "contracts/.env" ] && files+=("contracts/.env"); \
	need_key () { \
	  local k="$$1" found=0 val=""; \
	  for f in "$${files[@]}"; do \
	    if grep -qE "^$${k}=" "$$f" 2>/dev/null; then \
	      val="$$(grep -E "^$${k}=" "$$f" | head -n1 | cut -d= -f2-)"; \
	      if [ -n "$$val" ]; then found=1; break; fi; \
	    fi; \
	  done; \
	  if [ "$$found" -eq 0 ]; then \
	    echo "ERROR: $${k} tidak ditemukan atau kosong di $${files[*]}"; exit 1; \
	  fi; \
	}; \
	need_key "PRIVATE_KEY"; \
	adm_found=0; \
	for k in ADMIN_WALLET X402_ADMIN_WALLET; do \
	  for f in "$${files[@]}"; do \
	    if grep -qE "^$${k}=" "$$f" 2>/dev/null; then \
	      val="$$(grep -E "^$${k}=" "$$f" | head -n1 | cut -d= -f2-)"; \
	      if [ -n "$$val" ]; then \
	        adm_found=1; break 2; \
	      fi; \
	    fi; \
	  done; \
	done; \
	if [ "$$adm_found" -eq 0 ]; then \
	  echo "ERROR: ADMIN_WALLET atau X402_ADMIN_WALLET tidak ditemukan/empty di $${files[*]}"; exit 1; \
	fi; \
	need_env_root () { \
	  local k="$$1"; \
	  if ! grep -qE "^$${k}=" .env 2>/dev/null; then \
	    echo "ERROR: $${k} belum ada di .env"; exit 1; \
	  fi; \
	  val="$$(grep -E "^$${k}=" .env | head -n1 | cut -d= -f2-)"; \
	  if [ -z "$$val" ]; then \
	    echo "ERROR: $${k} kosong di .env"; exit 1; \
	  fi; \
	}; \
	case "$$NET" in \
	  polygonAmoyTestnet) \
	    need_env_root "AMOY_RPC_HTTP"; \
	    need_env_root "AMOY_CHAIN_ID"; \
	    ;; \
	  polygonMainnet) \
	    need_env_root "POLYGON_RPC_HTTP"; \
	    need_env_root "POLYGON_CHAIN_ID"; \
	    ;; \
	  megaTestnet) \
	    need_env_root "MEGA_RPC_HTTP"; \
	    need_env_root "MEGA_CHAIN_ID"; \
	    ;; \
	  "") \
	    echo "ERROR: NET belum diisi. Contoh: NET=polygonAmoyTestnet"; exit 1; \
	    ;; \
	  *) \
	    echo "ERROR: NET tidak dikenal: $$NET. Gunakan polygonAmoyTestnet | polygonMainnet | megaTestnet"; exit 1; \
	    ;; \
	esac'
endef

# ---------------------------
# Aliases singkat (opsional)
# gunakan: make TARGET=deployx402 amoy
# ---------------------------
.PHONY: amoy mainnet mega checkx402 estimatex402 deployx402 deployx402-mainnet showx402 verifyx402

amoy:
	@$(MAKE) $(TARGET) NET=polygonAmoyTestnet

mainnet:
	@$(MAKE) $(TARGET) NET=polygonMainnet

mega:
	@$(MAKE) $(TARGET) NET=megaTestnet

# ---------------------------
# Tasks
# ---------------------------
checkx402:
	@echo "==> Checking admin wallet balance on $(NET)"
	$(_check_env_base)
	@$(COMPOSE) $(DEPLOY_ENV_FILES) run --rm x402-deployer \
	  npx hardhat run --network $(NET) scripts/check_balance.js

estimatex402:
	@echo "==> Estimasi gas pada network: $(NET)"
	$(_check_env_base)
	@$(COMPOSE) $(DEPLOY_ENV_FILES) run --rm x402-deployer \
	  npx hardhat run --network $(NET) scripts/estimate_gas_cost.js

deployx402:
	@echo "==> Deploy X402Splitter ke network: $(NET)"
	$(_check_env_base)
	@tmpfile=$$(mktemp); \
	  $(COMPOSE) $(DEPLOY_ENV_FILES) run --rm x402-deployer \
	    npx hardhat run --network $(NET) scripts/deploy_x402.js | tee $$tmpfile; \
	  \
	  addr=""; \
	  if [ -f "contracts/deployed.json" ]; then \
	    if command -v jq >/dev/null 2>&1; then \
	      addr=$$(jq -r '.[-1].X402Splitter.address // empty' contracts/deployed.json 2>/dev/null | grep -E '^0x[a-fA-F0-9]{40}$$' || true); \
	    fi; \
	  fi; \
	  \
	  if [ -z "$$addr" ]; then \
	    addr=$$(grep -Eo '0x[a-fA-F0-9]{40}' $$tmpfile | tail -n1); \
	  fi; \
	  \
	  rm -f $$tmpfile; \
	  \
	  if [ -z "$$addr" ]; then \
	    echo "ERROR: Alamat kontrak tidak terdeteksi dari output deployment."; \
	    exit 1; \
	  fi; \
	  \
	  if ! echo "$$addr" | grep -qE '^0x[a-fA-F0-9]{40}$$'; then \
	    echo "ERROR: Format alamat tidak valid: $$addr"; \
	    exit 1; \
	  fi; \
	  \
	  echo ""; \
	  echo "==> Kontrak ter-deploy di: $$addr"; \
	  $(call _env_set,X402_CONTRACT_ADDRESS,$$addr); \
	  echo "Saved to .env: X402_CONTRACT_ADDRESS=$$addr"

deployx402-mainnet:
	@$(MAKE) deployx402 NET=polygonMainnet

showx402:
	@addr=$$(grep -E '^X402_CONTRACT_ADDRESS=' .env 2>/dev/null | cut -d= -f2- || echo 'N/A'); \
	echo "X402_CONTRACT_ADDRESS=$$addr"

# Optional: verify langsung (perlu POLYGONSCAN_API_KEY di .env)
verifyx402:
	@echo "==> Verifying X402Splitter pada $(NET)"
	$(_check_env_base)
	@ADDR=$$(grep -E '^X402_CONTRACT_ADDRESS=' .env 2>/dev/null | cut -d= -f2-); \
	if [ -z "$$ADDR" ]; then \
	  echo "ERROR: X402_CONTRACT_ADDRESS belum ada di .env"; \
	  exit 1; \
	fi; \
	ADMIN=$$(grep -E '^(ADMIN_WALLET|X402_ADMIN_WALLET)=' contracts/.env 2>/dev/null | head -n1 | cut -d= -f2- || \
	         grep -E '^(ADMIN_WALLET|X402_ADMIN_WALLET)=' .env 2>/dev/null | head -n1 | cut -d= -f2-); \
	if [ -z "$$ADMIN" ]; then \
	  echo "ERROR: ADMIN_WALLET atau X402_ADMIN_WALLET tidak ditemukan"; \
	  exit 1; \
	fi; \
	echo "Verifying contract at $$ADDR with admin $$ADMIN"; \
	$(COMPOSE) $(DEPLOY_ENV_FILES) run --rm x402-deployer \
	  npx hardhat verify --network $(NET) $$ADDR $$ADMIN
