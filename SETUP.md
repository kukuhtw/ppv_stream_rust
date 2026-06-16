# PPV Stream Setup Guide

This guide explains how to set up and run **PPV Stream** in two ways:

1. **Docker setup**: recommended for most developers.
2. **Non-Docker setup**: useful if you want to run PostgreSQL and the Rust app directly on your machine.

The instructions below are written in **English**, are intentionally step-by-step, and include the environment variables you need to get a working local instance.

---

## 1. What You Are Setting Up

PPV Stream is a Rust + PostgreSQL application with these main runtime dependencies:

- **Rust** for the backend application
- **PostgreSQL** for the database
- **FFmpeg** for video processing and HLS transcoding
- **Optional payment provider credentials** for Stripe, PayPal, Midtrans, Xendit, or X402

For local development, you can start with:

- local PostgreSQL
- local file storage
- no real payment gateway credentials
- optional seeded test users

---

## 2. Repository Layout You Should Know

Important folders and files:

- `src/` - Rust backend
- `public/` - HTML, CSS, and JavaScript frontend
- `sql/` - core SQL schema migrations
- `migrations/` - additional SQL migrations
- `docker-compose.yml` - Docker stack
- `Dockerfile` - app image build
- `Makefile` - helper commands for Docker workflow

---

## 3. Environment Variables

Create a `.env` file in the project root.

Use this minimal local-development example as your starting point:

```env
# Core application
DATABASE_URL=postgres://ppv:secret@localhost:5432/ppv_stream
DATABASE_URL_BUILD=postgres://ppv:secret@db:5432/ppv_stream
BIND=0.0.0.0:8080
BASE_URL=http://localhost:8080
HMAC_SECRET=change-this-to-a-long-random-secret
SESSION_TOKEN_TTL=3600
RUST_LOG=info

# Directories
STORAGE_DIR=storage
UPLOAD_DIR=storage
MEDIA_DIR=media
HLS_ROOT=hls_tmp
TMP_DIR=tmp
PUBLIC_DIR=public

# Upload and media
MAX_UPLOAD_BYTES=1073741824
ALLOW_EXTS=mp4,mkv,mov,webm
HLS_SEGMENT_SECONDS=2
HWACCEL=none
WATERMARK_FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf

# Currency
DOLLAR_USD_TO_RUPIAH=17000

# Revenue split
CREATOR_SPLIT_BP=9000

# Admin bootstrap
ADMIN_BOOTSTRAP_TOKEN=setup-token-123
ADMIN_BOOTSTRAP_EMAIL=admin@example.com
ADMIN_BOOTSTRAP_PASSWORD=ChangeMe123!

# Storage backend
STORAGE_BACKEND=local
STORAGE_LOCAL_PATH=storage

# Optional watcher
WATCHER_ENABLE=0

# Optional X402 placeholders
X402_CONTRACT_ADDRESS=
X402_ADMIN_WALLET=
X402_ADMIN_PRIVKEY=
X402_RPC_HTTP=
X402_RPC_WSS=
X402_CHAIN_ID=80002
X402_DEADLINE_SECS=900

# Optional fiat payment providers
PAYMENT_PLUGINS=
PAYMENT_DEFAULT_PROVIDER=

STRIPE_ENV=test
STRIPE_SECRET_KEY=
STRIPE_WEBHOOK_SECRET=

PAYPAL_ENV=sandbox
PAYPAL_CLIENT_ID=
PAYPAL_CLIENT_SECRET=
PAYPAL_WEBHOOK_ID=

MIDTRANS_ENV=sandbox
MIDTRANS_SERVER_KEY=
MIDTRANS_CLIENT_KEY=

XENDIT_ENV=test
XENDIT_SECRET_KEY=
XENDIT_WEBHOOK_TOKEN=

# Optional S3-compatible storage
S3_BUCKET=
S3_REGION=us-east-1
S3_ACCESS_KEY=
S3_SECRET_KEY=
S3_ENDPOINT=
S3_PATH_STYLE=false
S3_PUBLIC_URL=
```

Notes:

- `DATABASE_URL` is used by the running app.
- `DATABASE_URL_BUILD` is used by the Docker build because this project uses `sqlx` macros that may require database access during compilation.
- For local non-Docker development, `DATABASE_URL` must be reachable **before** you run `cargo build` or `cargo run`.

---

## 4. Prerequisites

### Option A: Docker workflow

Install:

- Docker Desktop or Docker Engine
- Docker Compose v2

### Option B: Non-Docker workflow

Install:

- Rust toolchain, preferably stable
- PostgreSQL 16 or compatible
- FFmpeg
- `psql` command-line client

Recommended Rust installation:

```bash
rustup default stable
rustup update
```

Check your tools:

```bash
rustc --version
cargo --version
psql --version
ffmpeg -version
```

---

## 5. Docker Setup (Recommended)

This is the easiest way to get the app running locally.

### Step 1: Clone the repository

```bash
git clone https://github.com/kukuhtw/ppv_stream_rust.git
cd ppv_stream_rust
```

### Step 2: Create `.env`

Create a `.env` file in the project root using the sample above.

For Docker, make sure these are correct:

```env
DATABASE_URL=postgres://ppv:secret@db:5432/ppv_stream
DATABASE_URL_BUILD=postgres://ppv:secret@db:5432/ppv_stream
```

Why:

- inside Docker Compose, the database hostname is `db`
- not `localhost`

### Step 3: Start PostgreSQL

```bash
make db-up
```

Wait until the database is healthy:

```bash
make wait-db
```

### Step 4: Run all migrations

```bash
make migrate
```

Important:

- `make migrate` applies both `./sql` and `./migrations`
- this is important because the app runtime only auto-applies `./sql`
- extra features such as wallet and affiliate tables live under `./migrations`

### Step 5: Build the app image

```bash
make build
```

If the build fails during SQLx macro expansion:

- confirm `DATABASE_URL_BUILD` points to a working PostgreSQL instance
- confirm the `db` service is already running

### Step 6: Start the app

```bash
make run-all
```

This command:

- starts the database if needed
- builds the app image
- waits for DB health
- starts the app container

### Step 7: Open the app

Open:

- App: `http://localhost:8080`
- Adminer: `http://localhost:8081`

To start Adminer if it is not already running:

```bash
make adminer-up
```

### Step 8: Seed test users (optional)

```bash
make seed
```

This creates 10 test users:

- `user01@example.com` / `Passw0rd01!`
- `user02@example.com` / `Passw0rd02!`
- ...
- `user10@example.com` / `Passw0rd10!`

### Step 9: Create or reset the admin account

This project provides an admin bootstrap route:

```text
http://localhost:8080/setup_admin?token=setup-token-123
```

Replace the token with your `ADMIN_BOOTSTRAP_TOKEN` value from `.env`.

The route will:

- create an admin user if it does not exist
- or promote/reset the configured admin user if it already exists

Login page:

```text
http://localhost:8080/public/admin/login.html
```

---

## 6. Non-Docker Setup

Use this if you want to run PostgreSQL and the Rust app directly on your machine.

### Step 1: Clone the repository

```bash
git clone https://github.com/kukuhtw/ppv_stream_rust.git
cd ppv_stream_rust
```

### Step 2: Install PostgreSQL and create a database

Create the database and user:

```sql
CREATE USER ppv WITH PASSWORD 'secret';
CREATE DATABASE ppv_stream OWNER ppv;
```

Grant privileges if needed:

```sql
GRANT ALL PRIVILEGES ON DATABASE ppv_stream TO ppv;
```

### Step 3: Create `.env`

For non-Docker local development, set:

```env
DATABASE_URL=postgres://ppv:secret@localhost:5432/ppv_stream
BASE_URL=http://localhost:8080
PUBLIC_DIR=public
STORAGE_BACKEND=local
STORAGE_LOCAL_PATH=storage
```

### Step 4: Create required local directories

The app creates directories automatically, but it is still useful to create them upfront:

```bash
mkdir -p storage media hls_tmp tmp
```

On Windows PowerShell:

```powershell
New-Item -ItemType Directory -Force storage, media, hls_tmp, tmp
```

### Step 5: Apply SQL migrations manually

This is important.

Because the Rust app auto-runs only the migrations under `./sql`, you should also apply all files under `./migrations` yourself before running the full app.

Run the `sql/` files first:

```bash
for f in sql/*.sql; do
  psql postgresql://ppv:secret@localhost:5432/ppv_stream -f "$f"
done
```

Then run the `migrations/` files:

```bash
for f in migrations/*.sql; do
  psql postgresql://ppv:secret@localhost:5432/ppv_stream -f "$f"
done
```

On Windows PowerShell:

```powershell
Get-ChildItem sql\*.sql | Sort-Object Name | ForEach-Object {
  psql "postgresql://ppv:secret@localhost:5432/ppv_stream" -f $_.FullName
}

Get-ChildItem migrations\*.sql | Sort-Object Name | ForEach-Object {
  psql "postgresql://ppv:secret@localhost:5432/ppv_stream" -f $_.FullName
}
```

### Step 6: Build the Rust application

```bash
cargo build
```

Important:

- the database must already be reachable here
- this project uses `sqlx::query!` macros, so compile-time database access may be required

### Step 7: Run the app

```bash
cargo run
```

The app will start at:

```text
http://localhost:8080
```

### Step 8: Seed test users (optional)

```bash
cargo run --bin seed_dummy
```

### Step 9: Bootstrap the admin user

Open:

```text
http://localhost:8080/setup_admin?token=setup-token-123
```

Then log in via:

```text
http://localhost:8080/public/admin/login.html
```

---

## 7. First-Time Verification Checklist

After the app starts, verify these pages:

### Public pages

- `http://localhost:8080/public/`
- `http://localhost:8080/public/browse.html`
- `http://localhost:8080/public/auth/register.html`
- `http://localhost:8080/public/auth/login.html`

### User pages

- `http://localhost:8080/public/dashboard.html`
- `http://localhost:8080/public/wallet.html`
- `http://localhost:8080/public/affiliate.html`

### Admin pages

- `http://localhost:8080/public/admin/login.html`
- `http://localhost:8080/public/admin/dashboard.html`

### Health endpoint

- `http://localhost:8080/health`

Expected result:

- the endpoint returns `ok`

---

## 8. Running With Local File Storage

The easiest storage mode is local storage.

Use:

```env
STORAGE_BACKEND=local
STORAGE_LOCAL_PATH=storage
```

This means:

- uploaded source files are stored locally
- processed media is stored locally
- no S3-compatible credentials are needed

Recommended for development:

- keep `STORAGE_BACKEND=local`
- add S3/MinIO/R2/B2 only when you actually need cloud storage

---

## 9. Running With Optional Payment Providers

You can start the app without payment provider credentials if you only want to test:

- registration
- login
- profile
- upload
- wallet UI
- affiliate UI
- chat UI
- admin UI

If you want to enable payment providers, add the correct env vars and list them in:

```env
PAYMENT_PLUGINS=stripe,paypal,midtrans,xendit,x402
PAYMENT_DEFAULT_PROVIDER=stripe
```

Only include the providers you have configured.

### Stripe

```env
STRIPE_ENV=test
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...
```

### PayPal

```env
PAYPAL_ENV=sandbox
PAYPAL_CLIENT_ID=...
PAYPAL_CLIENT_SECRET=...
PAYPAL_WEBHOOK_ID=...
```

### Midtrans

```env
MIDTRANS_ENV=sandbox
MIDTRANS_SERVER_KEY=...
MIDTRANS_CLIENT_KEY=...
```

### Xendit

```env
XENDIT_ENV=test
XENDIT_SECRET_KEY=...
XENDIT_WEBHOOK_TOKEN=...
```

### X402

```env
X402_CONTRACT_ADDRESS=0x...
X402_ADMIN_WALLET=0x...
X402_ADMIN_PRIVKEY=0x...
X402_RPC_HTTP=https://...
X402_RPC_WSS=wss://...
X402_CHAIN_ID=80002
X402_DEADLINE_SECS=900
```

If you need the smart contract deployment flow, also see:

- `contracts/guidance_smartcontract_deployment`
- `make deployx402`
- `make estimatex402`
- `make checkx402`

---

## 10. Useful Docker Commands

Start DB only:

```bash
make db-up
```

Run migrations:

```bash
make migrate
```

Build app:

```bash
make build
```

Run app:

```bash
make run-all
```

Tail app logs:

```bash
make logs
```

Tail DB logs:

```bash
make logs-db
```

See running containers:

```bash
make ps
```

Stop all services:

```bash
make stop
```

Stop and remove Compose network:

```bash
make down
```

Open shell in app container:

```bash
make sh
```

---

## 11. Common Problems and Fixes

### Problem: `cargo build` or Docker build fails with SQLx database errors

Cause:

- this project uses `sqlx::query!` macros
- those macros may require a live database connection during compilation

Fix:

- make sure PostgreSQL is already running
- make sure `DATABASE_URL` or `DATABASE_URL_BUILD` points to a reachable database
- for Docker, start `db` first

### Problem: The app starts but wallet/affiliate tables are missing

Cause:

- only `./sql` was applied
- `./migrations` was not applied

Fix:

- run `make migrate`
- or manually apply both `sql/*.sql` and `migrations/*.sql`

### Problem: Videos upload but playback/transcoding fails

Cause:

- FFmpeg is missing
- directories are not writable
- font path is invalid

Fix:

- install FFmpeg
- verify `storage`, `media`, `hls_tmp`, and `tmp`
- set `WATERMARK_FONT` explicitly if the default font path does not exist

### Problem: Admin login does not work

Fix:

1. confirm `ADMIN_BOOTSTRAP_TOKEN`, `ADMIN_BOOTSTRAP_EMAIL`, and `ADMIN_BOOTSTRAP_PASSWORD`
2. open `/setup_admin?token=...`
3. then log in at `/public/admin/login.html`

### Problem: Docker app cannot connect to PostgreSQL

Fix:

- inside Docker Compose, use `db` as the hostname
- not `localhost`

Correct:

```env
DATABASE_URL=postgres://ppv:secret@db:5432/ppv_stream
```

---

## 12. Recommended Development Flow

If you want the smoothest local experience, use this order:

### Docker workflow

1. create `.env`
2. run `make db-up`
3. run `make wait-db`
4. run `make migrate`
5. run `make build`
6. run `make run-all`
7. run `make seed`
8. open `/setup_admin?token=...`

### Non-Docker workflow

1. install PostgreSQL, FFmpeg, and Rust
2. create the database
3. create `.env`
4. apply `sql/*.sql`
5. apply `migrations/*.sql`
6. run `cargo build`
7. run `cargo run`
8. run `cargo run --bin seed_dummy`
9. open `/setup_admin?token=...`

---

## 13. Next Documents to Read

After setup, these are the best follow-up documents:

- [README.md](README.md)
- [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md)
- [PAYMENT.md](PAYMENT.md)
- [AFFILIATE.md](AFFILIATE.md)
- [WALLET.md](WALLET.md)
- [ADMIN_AUTHENTICATION.md](ADMIN_AUTHENTICATION.md)

