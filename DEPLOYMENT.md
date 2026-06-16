# PPV Stream Cloud Deployment Guide

This document explains how to deploy **PPV Stream** to cloud infrastructure in a detailed, step-by-step way.

It covers:

- **Docker deployment**
- **Non-Docker deployment**
- **DigitalOcean Droplet**
- **Google Cloud**
- **Microsoft Azure**
- **Cloudflare**
- **Vercel**

It also explains where some platforms are a **good fit**, where they are only a **partial fit**, and where they are **not recommended** for the full application.

---

## 1. What You Are Deploying

PPV Stream is a **stateful Rust web application** with these important runtime requirements:

- A Rust backend process
- PostgreSQL
- FFmpeg
- Writable storage for uploads, HLS output, temporary files, and wallet/payment artifacts
- Public HTTP or HTTPS access
- Optional webhook endpoints for payment gateways
- Optional blockchain RPC access for x402

Because of that, the easiest production deployment targets are:

- A VM or Droplet
- A cloud VM with Docker Compose
- A cloud VM running the app directly

The least suitable targets for the **full app** are:

- Vercel serverless
- Cloudflare Workers

Those platforms can still be used for **frontend/static delivery**, **DNS**, **CDN**, or **reverse proxy**, but not as the best place to run the full PPV Stream backend itself.

---

## 2. Recommended Production Architecture

For production, the most practical architecture is:

1. A Linux VM
2. PostgreSQL on the same VM or a managed PostgreSQL service
3. The PPV Stream app running via Docker Compose or systemd
4. Nginx or Caddy as the reverse proxy
5. HTTPS with Let's Encrypt
6. A real domain name
7. Object storage only if you intentionally move beyond local disk

Recommended baseline:

- OS: **Ubuntu 24.04 LTS**
- CPU: **2 vCPU minimum**
- RAM: **4 GB minimum**
- Disk: **60 GB minimum**, more if storing video locally

If you expect video uploads, HLS transcoding, and multiple concurrent viewers, prefer:

- 4 vCPU
- 8 GB RAM
- SSD storage

---

## 3. Before You Deploy

Prepare these items first:

- A domain name, for example `stream.example.com`
- A server or cloud account
- A PostgreSQL plan
- Payment provider credentials if needed
- SMTP credentials if you want email features
- Blockchain RPC and admin wallet values if you want x402

You should also decide:

- Will you use **Docker** or **non-Docker**?
- Will PostgreSQL run **on the same machine** or be **managed externally**?
- Will media be stored on **local disk** or **S3-compatible storage**?

For most teams:

- Use **Docker**
- Use **Ubuntu VM**
- Start with **local disk storage**
- Use **managed PostgreSQL** if budget allows

---

## 4. Production Environment Variables

Create a production `.env` file. Start from `.env.example`, then replace local values.

Important production values:

```env
DATABASE_URL=postgres://ppv:strong-password@127.0.0.1:5432/ppv_stream
DATABASE_URL_BUILD=postgres://ppv:strong-password@127.0.0.1:5432/ppv_stream
BIND=0.0.0.0:8080
BASE_URL=https://stream.example.com
HMAC_SECRET=replace-with-a-long-random-secret
SESSION_TOKEN_TTL=3600
RUST_LOG=info

STORAGE_BACKEND=local
STORAGE_LOCAL_PATH=storage
UPLOAD_DIR=storage
MEDIA_DIR=media
HLS_ROOT=hls_tmp
TMP_DIR=tmp
PUBLIC_DIR=public

MAX_UPLOAD_BYTES=1073741824
ALLOW_EXTS=mp4,mkv,mov,webm
HLS_SEGMENT_SECONDS=2
HWACCEL=none

DOLLAR_USD_TO_RUPIAH=17000
CREATOR_SPLIT_BP=9000

ADMIN_BOOTSTRAP_TOKEN=replace-this
ADMIN_BOOTSTRAP_EMAIL=admin@example.com
ADMIN_BOOTSTRAP_PASSWORD=replace-this

PAYMENT_PLUGINS=paypal,xendit
PAYMENT_DEFAULT_PROVIDER=paypal

PAYPAL_ENV=live
PAYPAL_CLIENT_ID=
PAYPAL_CLIENT_SECRET=
PAYPAL_WEBHOOK_ID=

XENDIT_ENV=live
XENDIT_SECRET_KEY=
XENDIT_WEBHOOK_TOKEN=

STRIPE_ENV=live
STRIPE_SECRET_KEY=
STRIPE_WEBHOOK_SECRET=

MIDTRANS_ENV=production
MIDTRANS_SERVER_KEY=
MIDTRANS_CLIENT_KEY=

WATCHER_ENABLE=0

X402_CONTRACT_ADDRESS=
X402_ADMIN_WALLET=
X402_ADMIN_PRIVKEY=
X402_RPC_HTTP=
X402_RPC_WSS=
X402_CHAIN_ID=80002
X402_DEADLINE_SECS=900
```

Production notes:

- `BASE_URL` must match your real public URL.
- Payment webhooks must point to the same public base URL.
- If you use Docker build with `sqlx` online mode, `DATABASE_URL_BUILD` must be reachable during build.
- Wallet payment and wallet transfer are enabled in `Admin > Settings > Payment Methods`.
- Provider availability is a combination of `.env` credentials and admin toggles stored in the database.

---

## 5. Shared Production Checklist

Use this checklist on any VM-based deployment.

### 5.1 Server preparation

1. Create the VM.
2. Open ports `80`, `443`, and optionally `22`.
3. SSH into the server.
4. Update packages.
5. Create an application directory such as `/opt/ppv_stream`.
6. Clone the repository.
7. Create `.env`.

### 5.2 PostgreSQL preparation

1. Install PostgreSQL or provision a managed instance.
2. Create a database named `ppv_stream`.
3. Create a database user.
4. Put the final database URL into `.env`.
5. Make sure the database is reachable from the app host.

### 5.3 Media and file system preparation

If using local storage, create directories:

```bash
mkdir -p /opt/ppv_stream/storage
mkdir -p /opt/ppv_stream/media
mkdir -p /opt/ppv_stream/hls_tmp
mkdir -p /opt/ppv_stream/tmp
```

### 5.4 Reverse proxy and SSL

In production, do not expose the app directly unless you are only testing.

Preferred setup:

- App listens on `127.0.0.1:8080` or internal network `0.0.0.0:8080`
- Nginx or Caddy serves HTTPS on `443`
- Domain points to the VM public IP

### 5.5 First-run application tasks

1. Start PostgreSQL.
2. Run all SQL in `sql/` and `migrations/`.
3. Start the app.
4. Open the site in the browser.
5. Log in as admin.
6. Go to `Admin > Settings > Payment Methods`.
7. Enable the payment methods you want.
8. Test user registration, upload, and payment flow.

---

## 6. Docker Deployment on a Linux VM

This is the most recommended deployment model.

### 6.1 Install Docker

On Ubuntu:

```bash
sudo apt update
sudo apt install -y ca-certificates curl gnupg
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
sudo chmod a+r /etc/apt/keyrings/docker.gpg
echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu \
  $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
  sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt update
sudo apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo usermod -aG docker $USER
newgrp docker
```

### 6.2 Clone the repository

```bash
sudo mkdir -p /opt/ppv_stream
sudo chown $USER:$USER /opt/ppv_stream
git clone <your-repo-url> /opt/ppv_stream
cd /opt/ppv_stream
```

### 6.3 Create the environment file

```bash
cp .env.example .env
nano .env
```

Set at minimum:

- `DATABASE_URL`
- `DATABASE_URL_BUILD`
- `BASE_URL`
- `HMAC_SECRET`
- payment credentials you need

### 6.4 Start database

```bash
make db-up
make wait-db
```

### 6.5 Run migrations

```bash
make migrate
```

### 6.6 Build and run the application

```bash
make build
make run-all
```

### 6.7 Check logs and health

```bash
make logs
make health
```

Expected test URL:

- `http://YOUR_SERVER_IP:8080`

### 6.8 Put Nginx in front

Install Nginx:

```bash
sudo apt install -y nginx
```

Create `/etc/nginx/sites-available/ppv_stream`:

```nginx
server {
    listen 80;
    server_name stream.example.com;

    client_max_body_size 2G;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Enable it:

```bash
sudo ln -s /etc/nginx/sites-available/ppv_stream /etc/nginx/sites-enabled/ppv_stream
sudo nginx -t
sudo systemctl reload nginx
```

### 6.9 Add HTTPS

```bash
sudo apt install -y certbot python3-certbot-nginx
sudo certbot --nginx -d stream.example.com
```

### 6.10 Final production tasks

1. Visit `https://stream.example.com`
2. Log in as admin
3. Enable payment methods
4. Configure SMTP
5. Configure wallet and affiliate behavior
6. Test webhook endpoints from each provider

---

## 7. Non-Docker Deployment on a Linux VM

Use this when you want direct OS-level control.

### 7.1 Install dependencies

On Ubuntu:

```bash
sudo apt update
sudo apt install -y curl build-essential pkg-config libssl-dev ffmpeg postgresql postgresql-contrib git
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
```

### 7.2 Install and configure PostgreSQL

```bash
sudo systemctl enable postgresql
sudo systemctl start postgresql
sudo -u postgres psql
```

Inside `psql`:

```sql
CREATE USER ppv WITH PASSWORD 'strong-password';
CREATE DATABASE ppv_stream OWNER ppv;
\q
```

### 7.3 Clone the repository

```bash
sudo mkdir -p /opt/ppv_stream
sudo chown $USER:$USER /opt/ppv_stream
git clone <your-repo-url> /opt/ppv_stream
cd /opt/ppv_stream
```

### 7.4 Create `.env`

```bash
cp .env.example .env
nano .env
```

Example production DB settings:

```env
DATABASE_URL=postgres://ppv:strong-password@127.0.0.1:5432/ppv_stream
DATABASE_URL_BUILD=postgres://ppv:strong-password@127.0.0.1:5432/ppv_stream
BASE_URL=https://stream.example.com
```

### 7.5 Run SQL schema and migrations

```bash
for f in $(find sql -maxdepth 1 -type f -name '*.sql' | sort -V); do
  psql "postgres://ppv:strong-password@127.0.0.1:5432/ppv_stream" -f "$f"
done

for f in $(find migrations -maxdepth 1 -type f -name '*.sql' | sort -V); do
  psql "postgres://ppv:strong-password@127.0.0.1:5432/ppv_stream" -f "$f"
done
```

### 7.6 Build and run

```bash
cargo build --release
./target/release/ppv_stream
```

### 7.7 Run it as a systemd service

Create `/etc/systemd/system/ppv_stream.service`:

```ini
[Unit]
Description=PPV Stream Rust
After=network.target postgresql.service

[Service]
Type=simple
WorkingDirectory=/opt/ppv_stream
EnvironmentFile=/opt/ppv_stream/.env
ExecStart=/opt/ppv_stream/target/release/ppv_stream
Restart=always
RestartSec=5
User=www-data
Group=www-data

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable ppv_stream
sudo systemctl start ppv_stream
sudo systemctl status ppv_stream
```

### 7.8 Add Nginx and HTTPS

Use the same Nginx and Certbot steps as in the Docker section.

---

## 8. DigitalOcean Droplet

DigitalOcean Droplets are one of the best fits for this project.

### 8.1 Best deployment choice

Recommended:

- **Ubuntu Droplet**
- **Docker deployment**

Good alternatives:

- Ubuntu Droplet with non-Docker systemd deployment
- Droplet + Managed PostgreSQL

### 8.2 Create the Droplet

1. Sign in to DigitalOcean.
2. Create a new Droplet.
3. Choose **Ubuntu 24.04 LTS**.
4. Pick at least **Basic / 2 GB RAM**, preferably **4 GB** or more.
5. Add your SSH key.
6. Allow HTTP and HTTPS.
7. Create the Droplet.

### 8.3 Point your domain

In DNS:

- `A` record: `stream.example.com -> DROPLET_IP`

### 8.4 Docker deployment on DigitalOcean

SSH to the Droplet and follow:

- Section `6. Docker Deployment on a Linux VM`

DigitalOcean-specific recommendation:

- Use **Managed PostgreSQL** if you do not want DB maintenance.
- If you use local PostgreSQL, keep it private on the VM.

### 8.5 Non-Docker deployment on DigitalOcean

SSH to the Droplet and follow:

- Section `7. Non-Docker Deployment on a Linux VM`

### 8.6 Optional managed database

If using DigitalOcean Managed PostgreSQL:

1. Create a managed PostgreSQL cluster.
2. Create a database and user.
3. Allow the Droplet as a trusted source.
4. Copy the connection string into `DATABASE_URL` and `DATABASE_URL_BUILD`.
5. Run migrations from the app host.

### 8.7 Storage guidance for DigitalOcean

You have two choices:

- Local disk on the Droplet
- S3-compatible object storage such as DigitalOcean Spaces

Use local disk first unless:

- you need large-scale media storage
- you need CDN-backed media delivery
- you are preparing multi-server deployment

---

## 9. Google Cloud

Google Cloud can host this app well, but the right product matters.

### 9.1 Best deployment choice

Recommended:

- **Compute Engine VM**

Possible for Docker only:

- **Cloud Run**, but only if you understand its stateless/container constraints

Recommended database:

- **Cloud SQL for PostgreSQL**

### 9.2 Compute Engine with Docker

1. Create a VM instance.
2. Use Ubuntu 24.04.
3. Allow HTTP and HTTPS.
4. SSH into the VM.
5. Install Docker.
6. Clone the repo.
7. Create `.env`.
8. Connect to PostgreSQL.
9. Run `make db-up` only if PostgreSQL is local.
10. Run `make migrate`.
11. Run `make build`.
12. Run `make run-all`.
13. Add Nginx and HTTPS.

If using **Cloud SQL**:

1. Create a PostgreSQL instance.
2. Create the database and user.
3. Allow connections from the VM.
4. Put the Cloud SQL host into `DATABASE_URL`.
5. Run migrations from the VM.

### 9.3 Compute Engine non-Docker

Use the same flow as:

- Section `7. Non-Docker Deployment on a Linux VM`

### 9.4 Cloud Run guidance

Cloud Run is not the best first deployment target for this app.

Reasons:

- local disk is ephemeral
- video processing workloads are heavier than a simple stateless API
- background processing and FFmpeg-heavy flows need more careful design
- persistent file paths need redesign or object storage

If you still want Cloud Run:

1. Move media storage to S3-compatible or Google Cloud Storage style abstraction if you adapt the storage layer.
2. Use Cloud SQL instead of local PostgreSQL.
3. Build a container image and push it to Artifact Registry.
4. Deploy to Cloud Run with the correct environment variables.
5. Expose HTTPS through Cloud Run.

This is a more advanced architecture and is not the easiest path for a first production launch.

---

## 10. Microsoft Azure

Azure is a good fit when you use a Linux VM or a container VM.

### 10.1 Best deployment choice

Recommended:

- **Azure Virtual Machine**

Recommended database:

- **Azure Database for PostgreSQL**

Possible but more advanced:

- **Azure Container Apps**
- **Azure App Service for Containers**

### 10.2 Azure VM with Docker

1. Create an Ubuntu VM.
2. Open ports `80` and `443`.
3. SSH into the VM.
4. Install Docker.
5. Clone the repo to `/opt/ppv_stream`.
6. Create `.env`.
7. Configure the PostgreSQL connection.
8. Run migrations.
9. Build and run the stack.
10. Add Nginx and HTTPS.

Use:

- Section `6. Docker Deployment on a Linux VM`

### 10.3 Azure VM non-Docker

Use:

- Section `7. Non-Docker Deployment on a Linux VM`

### 10.4 Azure managed PostgreSQL

1. Create an Azure Database for PostgreSQL instance.
2. Create the database and user.
3. Allow the VM outbound access to the DB.
4. Put the connection string in `.env`.
5. Run the schema and migrations from the VM.

### 10.5 Container Apps or App Service warning

These can work, but you must think carefully about:

- writable local storage
- large uploads
- FFmpeg execution
- HLS temporary files
- long-lived media processing

For that reason, a normal VM is the safer first production deployment.

---

## 11. Cloudflare

Cloudflare is useful around this project, but usually not as the place to run the full backend.

### 11.1 Best use cases for Cloudflare

Cloudflare is a good fit for:

- DNS
- CDN
- SSL termination
- DDoS protection
- WAF rules
- caching static public assets

### 11.2 What Cloudflare is not ideal for here

Cloudflare Workers are not the ideal runtime for the full PPV Stream app because this project needs:

- PostgreSQL
- FFmpeg
- writable filesystem behavior
- long-running media work
- full backend routing

### 11.3 Recommended Cloudflare architecture

Use Cloudflare in front of a VM deployment:

1. Deploy PPV Stream to DigitalOcean, Google Cloud VM, Azure VM, or another Linux server.
2. Put Cloudflare DNS in front of the domain.
3. Enable HTTPS and proxy mode.
4. Configure caching carefully for public static files only.
5. Do not aggressively cache authenticated pages or dynamic payment routes.

### 11.4 Cloudflare R2 possibility

If you later extend storage support, Cloudflare R2 could be used as object storage for media assets.

That is an architectural extension, not a drop-in full deployment target for the current app runtime.

---

## 12. Vercel

Vercel is excellent for frontend hosting, but not the best target for the entire PPV Stream stack.

### 12.1 Why Vercel is not recommended for the full app

The full project depends on:

- Rust backend runtime
- PostgreSQL
- FFmpeg
- writable directories
- long-running or heavy media processing

Vercel is optimized for:

- frontend apps
- serverless APIs
- edge delivery

That makes it a poor fit for the full PPV Stream backend as currently designed.

### 12.2 Recommended Vercel usage

Use Vercel only for a split architecture if you intentionally redesign deployment:

- frontend on Vercel
- Rust backend on a VM or container platform
- PostgreSQL on managed DB
- media on object storage

### 12.3 Current practical recommendation

For this repo in its current structure:

- Do **not** deploy the full app directly to Vercel
- Deploy the backend to a VM
- Optionally move only static landing pages or marketing pages to Vercel

---

## 13. Payment Provider Production Notes

For PayPal, Stripe, Xendit, and Midtrans:

1. Use live or production credentials, not sandbox keys.
2. Set provider webhook URLs to your real HTTPS domain.
3. Verify signatures or webhook tokens as required by each provider.
4. After login as admin, enable the provider in `Admin > Settings > Payment Methods`.

Example webhook patterns:

- PayPal: `https://stream.example.com/api/pay/paypal/webhook`
- Stripe: `https://stream.example.com/api/pay/stripe/webhook`
- Xendit: `https://stream.example.com/api/pay/xendit/webhook`
- Midtrans: `https://stream.example.com/api/pay/midtrans/webhook`

Important:

- Even if credentials exist in `.env`, the provider should still be enabled in the admin settings UI.
- Wallet payment and wallet transfer are also controlled from the admin settings UI.

---

## 14. x402 Blockchain Deployment Notes

If you want x402 enabled in production:

1. Deploy the smart contract.
2. Put the contract address in `.env`.
3. Set the correct RPC endpoints.
4. Set the admin wallet and private key.
5. Confirm the chain ID matches the network.
6. Enable x402 in `Admin > Settings > Payment Methods`.

Useful commands:

```bash
make checkx402
make estimatex402
make deployx402
make showx402
```

If you expose x402 to real users, make sure:

- the admin wallet has sufficient gas
- RPC endpoints are stable
- your public app URL is reachable for confirmation callbacks

---

## 15. Backup and Operations

For production, do not stop at "the app is running." You also need operations.

### 15.1 Back up PostgreSQL

At minimum, schedule:

```bash
pg_dump "postgres://ppv:strong-password@127.0.0.1:5432/ppv_stream" > backup.sql
```

### 15.2 Back up uploaded media

If using local disk, back up:

- `storage/`
- `media/`
- `hls_tmp/` if you rely on it operationally
- `.env`

### 15.3 Monitor logs

Docker:

```bash
make logs
```

Non-Docker:

```bash
sudo journalctl -u ppv_stream -f
```

### 15.4 Monitor disk usage

This app processes video, so disk usage matters.

Check regularly:

```bash
df -h
du -sh /opt/ppv_stream/storage
```

---

## 16. Platform Recommendation Summary

### Best overall choices

1. **DigitalOcean Droplet + Docker**
2. **Google Compute Engine + Docker**
3. **Azure VM + Docker**

### Best if you want the simplest operations

1. VM + Docker Compose
2. Managed PostgreSQL
3. Nginx + Let's Encrypt

### Platforms to use only partially

- **Cloudflare**: DNS, CDN, SSL, proxy, WAF
- **Vercel**: static frontend or marketing pages only

### Not recommended as first full deployment target

- Cloudflare Workers for the whole app
- Vercel for the whole app
- highly serverless runtimes without redesigning storage and media processing

---

## 17. Suggested First Production Path

If you want one clear recommendation, use this:

1. Create an Ubuntu Droplet on DigitalOcean.
2. Install Docker.
3. Clone this repo.
4. Copy `.env.example` to `.env`.
5. Fill production values.
6. Run `make db-up`.
7. Run `make migrate`.
8. Run `make build`.
9. Run `make run-all`.
10. Add Nginx.
11. Add Let's Encrypt.
12. Point your domain.
13. Log in as admin.
14. Enable payment methods.
15. Test registration, upload, wallet, affiliate, and payment flows.

That is the most direct and reliable first cloud deployment path for this repository.
