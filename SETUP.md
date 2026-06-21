# PPV Stream Setup Guide

This guide explains how to install and run PPV Stream Rust for local development or self hosted deployment.

PPV Stream is a Rust, Axum, PostgreSQL, FFmpeg, HTML, and JavaScript application. Start with PostgreSQL and local storage first. Enable payment providers, object storage, X402, and federation only after the base app is working.

## 1. Runtime Components

Core components:

* Rust backend application
* PostgreSQL database
* FFmpeg for video processing and HLS generation
* Local or S3 compatible storage
* Browser frontend under `public/`

Optional components:

* Internal wallet
* PayPal, Xendit, Stripe, Midtrans, and X402 payments
* SMTP email delivery
* Storage migration workflow
* ActivityPub federation

## 2. Important Repository Paths

| Path | Purpose |
| --- | --- |
| `src/` | Rust backend source code |
| `public/` | HTML, CSS, and JavaScript frontend |
| `sql/` | Core SQL schema |
| `migrations/` | Additional SQL migrations |
| `contracts/` | X402 smart contract workspace |
| `docs/federation-env-vars.md` | Federation environment variable reference |
| `FEDERATED_LEARN.md` | Federation concept guide |
| `FEDERATED_IMPLEMENTATION.md` | Federation implementation notes |
| `docker-compose.yml` | Docker stack |
| `Dockerfile` | Application container build |
| `Makefile` | Helper commands |

## 3. Environment File

Create a `.env` file in the project root:

```bash
cp .env.example .env
```

Configure these groups first:

* database connection
* bind address and base URL
* session signing value
* local storage paths
* upload and media settings
* admin bootstrap values
* default payment provider list

For Docker Compose, the PostgreSQL host should be `db`. For non Docker local development, the PostgreSQL host is usually `localhost`.

## 4. Prerequisites

Docker workflow:

* Docker Desktop or Docker Engine
* Docker Compose v2

Non Docker workflow:

* Rust stable toolchain
* PostgreSQL 16 or compatible
* FFmpeg
* `psql` command line client

Check tools:

```bash
rustc --version
cargo --version
psql --version
ffmpeg -version
```

## 5. Docker Setup

Clone repository:

```bash
git clone https://github.com/kukuhtw/ppv_stream_rust.git
cd ppv_stream_rust
```

Create environment file:

```bash
cp .env.example .env
```

Start PostgreSQL, run migrations, build, and run:

```bash
make db-up
make wait-db
make migrate
make build
make run-all
```

Open the app:

```text
http://localhost:8080
```

Seed test users when needed:

```bash
make seed
```

Create or reset the admin account by opening the admin bootstrap URL configured in your environment file, then login through:

```text
http://localhost:8080/public/admin/login.html
```

## 6. Non Docker Setup

Clone repository:

```bash
git clone https://github.com/kukuhtw/ppv_stream_rust.git
cd ppv_stream_rust
```

Create the PostgreSQL role and database according to your local PostgreSQL policy.

Create local directories:

```bash
mkdir -p storage media hls_tmp tmp
```

Apply core SQL files:

```bash
for f in sql/*.sql; do
  psql "$DATABASE_URL" -f "$f"
done
```

Apply additional migrations:

```bash
for f in migrations/*.sql; do
  psql "$DATABASE_URL" -f "$f"
done
```

Build and run:

```bash
cargo build
cargo run
```

Seed test users when needed:

```bash
cargo run --bin seed_dummy
```

Open the app:

```text
http://localhost:8080
```

## 7. X402 Blockchain Payment Setup

X402 is optional. Skip this section if you only need wallet or fiat payment first.

To enable X402, configure:

* deployed `X402Splitter` smart contract address
* admin wallet address
* backend invoice signing key
* HTTP RPC endpoint
* WebSocket RPC endpoint if the optional watcher is used
* `PAYMENT_PLUGINS` including `x402`

References:

* `contracts/contracts/X402Splitter.sol`
* `contracts/guidance_smartcontract_deployment`
* [PAYMENT.md](PAYMENT.md)

Enable the optional watcher only after the X402 WebSocket RPC endpoint and contract address are configured correctly.

## 8. Federation Setup

Federation is optional and disabled by default. In this project, federation means independent PPV Stream instances can exchange public identity and public index metadata while purchase, entitlement, wallet, media, HLS playback, and watermarking stay on the origin server.

Read these documents before enabling federation:

* [docs/federation-env-vars.md](docs/federation-env-vars.md) for exact environment variables
* [FEDERATED_LEARN.md](FEDERATED_LEARN.md) for the business and security model
* [FEDERATED_IMPLEMENTATION.md](FEDERATED_IMPLEMENTATION.md) for implementation details
* [docs/fed-install.md](docs/fed-install.md) for the short install note

Minimum federation preparation:

1. Start from a working PPV Stream instance.
2. Configure a canonical public base URL.
3. Use HTTPS for any public federation instance.
4. Enable federation in the environment file.
5. Configure the federation domain.
6. Configure federation admin access before using federation admin endpoints.
7. Restart the application.
8. Test discovery before connecting to another instance.

Docker restart:

```bash
make run-all
```

Non Docker restart:

```bash
cargo run
```

Important federation rules:

* `http://localhost` is acceptable only for local development.
* Keep the application signing configuration stable after federation actor keys are generated.
* Keep server clocks synchronized with NTP.
* Do not federate media files, HLS manifests, playback sessions, payment records, wallet balances, or private user data.
* Federation is index only. The origin server remains responsible for checkout, access control, streaming, watermarking, and revenue settlement.

Federation smoke test:

1. Start the app with federation enabled.
2. Confirm the public base URL resolves to the running instance.
3. Confirm the federation domain matches the public hostname.
4. Test WebFinger or federation discovery routes according to [FEDERATED_IMPLEMENTATION.md](FEDERATED_IMPLEMENTATION.md).
5. Confirm public profile and public video metadata are discoverable without exposing private payment or playback data.

## 9. First Time Verification Checklist

Public pages:

* `http://localhost:8080/public/`
* `http://localhost:8080/public/browse.html`
* `http://localhost:8080/public/auth/register.html`
* `http://localhost:8080/public/auth/login.html`

User pages:

* `http://localhost:8080/public/dashboard.html`
* `http://localhost:8080/public/wallet.html`
* `http://localhost:8080/public/affiliate.html`

Admin pages:

* `http://localhost:8080/public/admin/login.html`
* `http://localhost:8080/public/admin/dashboard.html`

Health endpoint:

```text
http://localhost:8080/health
```

Expected result:

```text
ok
```

## 10. Local File Storage

Default local storage keeps uploaded files and processed media on local disk.

For S3, MinIO, Cloudflare R2, Backblaze B2, and migration workflow, read:

* [STORAGE_MIGRATION.md](STORAGE_MIGRATION.md)
* [STORAGE_ADMIN_MOCKUP.md](STORAGE_ADMIN_MOCKUP.md)

## 11. Fiat Payment Provider Setup

Fiat providers are optional and can be enabled one by one.

Configure provider variables in `.env`, then restart the app after changing provider credentials.

## 12. Recommended Setup Order

1. Start PostgreSQL.
2. Run all migrations.
3. Start the Rust app.
4. Bootstrap the admin account.
5. Verify registration, login, upload, and playback.
6. Use local storage first.
7. Enable internal wallet payment and wallet transfer from admin settings.
8. Enable PayPal or Xendit if needed.
9. Add X402 testnet payment if needed.
10. Add federation only after the base instance is stable and reachable through a canonical URL.

## 13. Useful Docker Commands

```bash
make db-up
make wait-db
make migrate
make build
make run-all
make seed
make logs
make logs-db
make ps
make stop
make down
make sh
```

## 14. Common Problems and Fixes

### Cargo build or Docker build fails with SQLx database errors

* Make sure PostgreSQL is already running.
* Make sure database environment variables point to a reachable database.
* For Docker, use `db` as the hostname.

### Wallet or affiliate tables are missing

* Run `make migrate`.
* Or manually apply both `sql/*.sql` and `migrations/*.sql`.

### Video upload works but playback or transcoding fails

* Install FFmpeg.
* Confirm `storage`, `media`, `hls_tmp`, and `tmp` are writable.
* Set the watermark font path explicitly if the default font path does not exist.

### Admin login does not work

1. Confirm bootstrap environment variables.
2. Open `/setup_admin?token=...`.
3. Login at `/public/admin/login.html`.

### Federation does not work

* Confirm federation is enabled.
* Confirm the federation base URL is the canonical public URL.
* Confirm the federation domain is only the hostname.
* Confirm the signing configuration has not changed after actor keys were created.
* Confirm federation admin access is configured for admin endpoints.
* Review [docs/federation-env-vars.md](docs/federation-env-vars.md).

## 15. Next Documents to Read

After setup, read:

* [README.md](README.md)
* [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md)
* [PAYMENT.md](PAYMENT.md)
* [AFFILIATE.md](AFFILIATE.md)
* [WALLET.md](WALLET.md)
* [ADMIN_AUTHENTICATION.md](ADMIN_AUTHENTICATION.md)
* [docs/federation-env-vars.md](docs/federation-env-vars.md)
* [FEDERATED_LEARN.md](FEDERATED_LEARN.md)
* [FEDERATED_IMPLEMENTATION.md](FEDERATED_IMPLEMENTATION.md)
* [docs/fed-install.md](docs/fed-install.md)
