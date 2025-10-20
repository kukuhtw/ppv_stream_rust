APP_NAME       ?= ppv_stream
IMAGE_TAG      ?= $(APP_NAME):dev
BUILD_REV      ?= $(shell date +%s)
COMPOSE        ?= docker compose

.PHONY: help db-up db-down db-reset db-shell db-psql migrate seed build rebuild run run-all logs logs-db logs-all stop down sh adminer-up adminer-open ps health adminer-check clean wait-db

help:
	@echo ""
	@echo "📦 MAKEFILE COMMANDS — Panduan Penggunaan"
	@echo "====================================================="
	@echo "💾 DATABASE MANAGEMENT:"
	@echo "  db-up           : Menjalankan container Postgres (service db)."
	@echo "  db-down         : Menghentikan container database tanpa menghapus data."
	@echo "  db-reset        : Menghapus volume pgdata, membuat ulang DB, lalu menjalankan migrasi."
	@echo "  db-shell        : Masuk ke shell di dalam container database (bash)."
	@echo "  db-psql         : Masuk ke PostgreSQL CLI (psql) di container database."
	@echo "  wait-db         : Menunggu hingga database berstatus healthy (maks 30 detik)."
	@echo ""
	@echo "🗂️  DATABASE MIGRATION & SEED:"
	@echo "  migrate         : Menjalankan semua file SQL di folder ./sql ke dalam database."
	@echo "                    Urutan file diurutkan otomatis (001_init.sql, 002_xxx.sql, dst)."
	@echo "  seed            : Menjalankan binary 'seed_dummy' di dalam container app untuk membuat"
	@echo "                    10 user dummy dengan password Argon2 hash (user01..user10)."
	@echo "                    Contoh login: user03@example.com / Passw0rd03!"
	@echo ""
	@echo "⚙️  BUILD & DEPLOYMENT:"
	@echo "  build           : Build image app menggunakan cache (lebih cepat untuk development)."
	@echo "  rebuild         : Build ulang image tanpa cache (fresh build), lalu menjalankan app."
	@echo "  run             : Menjalankan seluruh service di docker-compose (db, app, adminer, dst)."
	@echo "  run-all         : Menjalankan database lalu aplikasi setelah DB sehat."
	@echo ""
	@echo "🔍 LOGS & MONITORING:"
	@echo "  logs            : Menampilkan log container aplikasi (realtime)."
	@echo "  logs-db         : Menampilkan log container database."
	@echo "  logs-all        : Menampilkan semua log dari semua service."
	@echo "  ps              : Menampilkan daftar container aktif dan statusnya."
	@echo "  health          : Melakukan pengecekan endpoint /health di port 8080."
	@echo ""
	@echo "🧰 ADMIN & MAINTENANCE:"
	@echo "  sh              : Masuk ke shell container app (ppv_stream_app)."
	@echo "  stop            : Menghentikan semua container tanpa menghapusnya."
	@echo "  down            : Menghentikan semua container dan menghapus network."
	@echo "  clean           : Menghapus image docker app ($(IMAGE_TAG)) dari lokal."
	@echo ""
	@echo "🧑‍💻 ADMINER (Database Web UI):"
	@echo "  adminer-up      : Menjalankan container Adminer di port 8081."
	@echo "  adminer-open    : Membuka Adminer di browser (http://localhost:8081)."
	@echo "  adminer-check   : Mengecek apakah Adminer sudah berjalan (HTTP HEAD)."
	@echo ""
	@echo "📖 CONTOH ALUR PENGGUNAAN:"
	@echo "  1. make db-up         → start database"
	@echo "  2. make migrate       → apply semua file SQL"
	@echo "  3. make build         → build aplikasi"
	@echo "  4. make run-all       → jalankan app + db"
	@echo "  5. make seed          → isi user dummy (login siap)"
	@echo "  6. make logs          → lihat log app berjalan"
	@echo ""
	@echo "====================================================="

# ----- Database -----
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

# Tunggu DB sehat
wait-db:
	@echo "==> waiting for db health..."
	@for i in $$(seq 1 30); do \
	  S=$$(docker inspect -f '{{.State.Health.Status}}' ppv_stream_db 2>/dev/null || echo "unknown"); \
	  echo "db health: $$S"; \
	  if [ "$$S" = "healthy" ]; then exit 0; fi; \
	  sleep 1; \
	done; \
	echo "DB not healthy in time"; exit 1

# ----- Migrations -----
migrate:
	@echo "==> Apply migrations dari ./sql ke database..."
	@ls -1 sql/*.sql >/dev/null 2>&1 || { echo "No SQL files in ./sql"; exit 0; }
	@for f in $$(find sql -maxdepth 1 -type f -name '*.sql' | sort -V); do \
	  echo "-> $$f"; \
	  docker exec -i ppv_stream_db psql -v ON_ERROR_STOP=1 -U ppv -d ppv_stream -f - < $$f || exit 1; \
	done
	@echo "==> Migrations selesai."
	
seed:
	@echo "==> Seeding 10 dummy users via binary..."
	@docker exec \
	  -e RUST_LOG=info \
	  -e DATABASE_URL=postgres://ppv:secret@db:5432/ppv_stream \
	  ppv_stream_app /usr/local/bin/seed_dummy || (echo "seed failed"; exit 1)

# ----- App lifecycle -----
# Build pakai cache (lebih cepat untuk dev). Gunakan 'rebuild' untuk no-cache.
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
