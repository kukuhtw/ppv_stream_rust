# PPV Stream Rust

Open source, self hosted, white label video commerce platform built with Rust, Axum, PostgreSQL, FFmpeg, and a browser based HTML and JavaScript frontend.

PPV Stream Rust lets creators upload videos, set prices, sell access through multiple payment methods, stream protected HLS video, manage internal wallet balances, run affiliate programs, chat with users, and optionally federate public platform data between independent installations.

## Project status

The repository is an actively developed reference implementation. Core features are implemented, but production deployment still requires operator review, secure secrets, provider credentials, HTTPS, database backups, monitoring, legal compliance, and infrastructure hardening.

Current package version: `0.2.0`

## Main capabilities

### Video commerce

* Creator video upload with extension checks, MIME validation, upload size limits, temporary files, and atomic rename
* PostgreSQL backed video catalog and ownership records
* Configurable video price
* Manual allowlist access and automatic access after successful purchase
* Creator profile and public profile endpoints

### Streaming and content protection

* FFmpeg based MP4 fast start processing
* Adaptive HLS output with 240p, 360p, and 480p renditions when supported by the source
* Per viewer HLS sessions
* Moving username and timestamp watermark
* HLS playlists and segments delivered with no store caching

Watermarking and session scoped delivery discourage casual redistribution. They do not make screen recording or content extraction impossible.

### Payments

* Internal wallet payment
* X402 EVM payment flow
* Stripe
* PayPal
* Midtrans
* Xendit
* Payment provider plugin registry
* Provider confirmation and webhook endpoints
* Configurable creator and platform revenue split

Payment providers are optional. Only providers enabled and configured by the operator are available at runtime.

### Wallet

* Balance lookup
* Deposit request
* Withdrawal request
* Peer to peer transfer
* Video purchase with wallet balance
* Admin approval, completion, and rejection workflow
* Wallet transaction ledger

The wallet is an internal application ledger. It is not a bank account, licensed e money product, or blockchain wallet.

### Affiliate system

* Per video affiliate settings
* Referral links using `?ref=USERNAME`
* Affiliate commission processing
* Affiliate earnings and summary endpoints
* Admin commission view

### Storage

* Local disk storage
* S3 compatible object storage through the storage plugin registry
* MinIO, AWS S3, Cloudflare R2, Backblaze B2, and compatible services
* Admin storage settings
* Connection test
* Background migration jobs
* Cancel, resume, retry tracking, and item level inspection

### Chat

* Search users available for chat
* List conversations
* Direct conversations
* Support conversation
* List and send messages

### Federation

The codebase includes optional federation support and an optional delivery worker. Federation is controlled through runtime configuration and is disabled unless explicitly enabled.

See [FEDERATED_LEARN.md](FEDERATED_LEARN.md) for the project model and trust boundaries.

### Administration and security controls

* User and admin authentication
* Argon2 password hashing
* HMAC SHA256 signed session cookies
* Admin role checks
* Browser CSRF guard
* Basic rate limiting middleware
* Security headers middleware
* SMTP settings and email notifications
* Initial admin bootstrap token

See [SECURITY.md](SECURITY.md) before exposing the platform to the internet.

## Documentation index

| Document | Purpose |
| --- | --- |
| [SETUP.md](SETUP.md) | Local setup, Docker setup, migrations, configuration, and startup |
| [DEPLOYMENT.md](DEPLOYMENT.md) | Deployment guidance for common infrastructure providers |
| [SECURITY.md](SECURITY.md) | Security model, hardening guidance, and remaining risks |
| [DISCLAIMER.md](DISCLAIMER.md) | Legal and operational disclaimer |
| [GLOSSARY.md](GLOSSARY.md) | Business and technical terminology |
| [ERD.md](ERD.md) | Database entities, relationships, and business rules |
| [DATA_FLOW.md](DATA_FLOW.md) | End to end application flows |
| [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md) | Source code and module reference |
| [PAYMENT.md](PAYMENT.md) | Wallet, X402, and fiat payment flows |
| [PAYMENT_PLUGIN_ARCHITECTURE.md](PAYMENT_PLUGIN_ARCHITECTURE.md) | Payment provider extension model |
| [WALLET.md](WALLET.md) | Internal wallet design and operations |
| [AFFILIATE.md](AFFILIATE.md) | Referral and commission system |
| [ADMIN_AUTHENTICATION.md](ADMIN_AUTHENTICATION.md) | Admin login and authorization |
| [STORAGE_MIGRATION.md](STORAGE_MIGRATION.md) | Storage migration operations |
| [STORAGE_ADMIN_MOCKUP.md](STORAGE_ADMIN_MOCKUP.md) | Storage administration user interface model |
| [FEDERATED_LEARN.md](FEDERATED_LEARN.md) | Federation concepts and boundaries |
| [RUST_CONCEPTS_FOR_BEGINNERS.md](RUST_CONCEPTS_FOR_BEGINNERS.md) | Rust concepts used by this repository |
| [VISION.md](VISION.md) | Product vision |
| [updated.md](updated.md) | Feature and architecture changelog |
| [DOCS_STATUS.md](DOCS_STATUS.md) | Documentation scope, verification date, and maintenance rules |

## Runtime architecture

```text
Browser
  |
  | HTTP and JSON
  v
Rust Axum application
  |-- authentication and sessions
  |-- video catalog and upload
  |-- FFmpeg worker
  |-- protected playback
  |-- wallet and affiliate logic
  |-- payment plugin registry
  |-- storage plugin registry
  |-- chat
  |-- optional federation
  |
  |-- PostgreSQL
  |-- local or S3 compatible storage
  |-- SMTP server
  |-- optional EVM network
  |-- optional payment providers
```

## Important routes

### Authentication

| Method | Route | Purpose |
| --- | --- | --- |
| POST | `/auth/register` | Register user |
| POST | `/auth/login` | Login user |
| POST | `/auth/logout` | Logout user |
| POST | `/api/change_password` | Change user password |
| POST | `/admin/login` | Login administrator |
| POST | `/admin/logout` | Logout administrator |
| POST | `/admin/change_password` | Change administrator password |

### Video and playback

| Method | Route | Purpose |
| --- | --- | --- |
| POST | `/api/upload` | Upload a video |
| GET | `/api/videos` | Browse videos |
| GET | `/api/my_videos` | List videos owned by the current user |
| POST | `/api/video_update` | Update video metadata |
| POST | `/api/allow` | Grant manual playback access |
| GET | `/api/request_play` | Request an authorized playback session |
| GET | `/hls/:session/:file` | Deliver session scoped HLS files |

### Payment

| Method | Route | Purpose |
| --- | --- | --- |
| GET | `/api/pay/all_options` | Return wallet, X402, and fiat choices |
| GET | `/api/pay/providers` | List active payment plugins |
| POST | `/api/pay/start` | Start payment with the default provider |
| POST | `/api/pay/confirm` | Confirm payment with the default provider |
| POST | `/api/pay/:provider/start` | Start provider payment |
| POST | `/api/pay/:provider/confirm` | Confirm provider payment |
| POST | `/api/pay/:provider/webhook` | Receive provider webhook |
| POST | `/api/pay/x402/start` | Start X402 invoice |
| POST | `/api/pay/x402/confirm` | Confirm X402 transaction |

### Wallet and affiliate

| Method | Route | Purpose |
| --- | --- | --- |
| GET | `/api/wallet/balance` | Wallet balance |
| GET | `/api/wallet/transactions` | Wallet history |
| POST | `/api/wallet/deposit` | Create deposit request |
| POST | `/api/wallet/withdraw` | Create withdrawal request |
| POST | `/api/wallet/transfer` | Transfer balance |
| POST | `/api/wallet/pay` | Buy video with wallet balance |
| GET and POST | `/api/affiliate/settings` | Read or update affiliate settings |
| GET | `/api/affiliate/summary` | Affiliate summary |
| GET | `/api/affiliate/link` | Generate referral link |
| GET | `/api/affiliate/earnings` | Affiliate earnings |
| GET | `/api/affiliate/program` | Public program information |

### Chat

| Method | Route | Purpose |
| --- | --- | --- |
| GET | `/api/chat/users` | Search chat users |
| GET | `/api/chat/conversations` | List conversations |
| POST | `/api/chat/conversations/support` | Open or retrieve support conversation |
| POST | `/api/chat/conversations/direct` | Start direct conversation |
| GET and POST | `/api/chat/conversations/:id/messages` | Read or send messages |

### Administration

| Method | Route | Purpose |
| --- | --- | --- |
| GET | `/admin/data` | Core administration data |
| GET | `/admin/payments` | Fiat payment records |
| POST | `/admin/payments/:uid/disburse` | Trigger supported disbursement |
| GET and POST | `/admin/payment_settings` | Payment settings |
| GET and POST | `/admin/storage_settings` | Storage settings |
| POST | `/admin/storage_settings/test` | Test storage configuration |
| GET and POST | `/admin/storage_migrations` | List or start migration jobs |
| POST | `/admin/storage_migrations/:id/cancel` | Cancel migration job |
| GET | `/admin/storage_migrations/:id/items` | Inspect migration items |
| GET and POST | `/admin/smtp` | SMTP settings |
| GET | `/admin/wallet/transactions` | Wallet administration |
| GET | `/admin/affiliate/commissions` | Affiliate commission administration |

## Project structure

```text
contracts/                 Solidity contract for X402 split payments
migrations/                Incremental PostgreSQL migrations
public/                    HTML, JavaScript, CSS, user pages, and admin pages
sql/                       Initial core database schema
src/
  commission.rs            Affiliate commission logic
  config.rs                Environment configuration
  db.rs                    PostgreSQL connection pool
  email.rs                 SMTP email delivery
  federation/              Federation routes and delivery worker
  ffmpeg.rs                Media processing helpers
  handlers/                HTTP request handlers
  middleware/              Security headers, CSRF guard, and rate limiting
  payment_settings.rs      Payment configuration persistence
  plugins/payment/         Payment provider plugins
  plugins/storage/         Storage provider plugins
  sessions.rs              Session signing and validation
  storage_settings.rs      Storage settings and migration support
  validators.rs            Input validation
  worker.rs                Background video processing
```

## Quick start

Read [SETUP.md](SETUP.md) for the complete instructions.

Typical development flow:

```bash
make db-up
make migrate
make build
make run
```

The default application address is normally:

```text
http://localhost:8080
```

A health check is available at:

```text
GET /health
```

## Minimum production checklist

1. Replace every development secret and sample credential.
2. Use HTTPS behind a trusted reverse proxy.
3. Configure a strong `HMAC_SECRET` and protect all provider secrets.
4. Restrict the admin bootstrap token and disable bootstrap access after use.
5. Configure PostgreSQL backups and test restoration.
6. Configure object storage durability or persistent local volumes.
7. Validate payment webhook signatures and provider production settings.
8. Review wallet, payout, tax, privacy, and content moderation obligations in the deployment jurisdiction.
9. Add monitoring, alerting, log retention, and incident procedures.
10. Run security and load testing before production traffic.

## Demonstration videos

* https://www.youtube.com/watch?v=WOsDwBcD03A
* https://www.youtube.com/watch?v=IuSjkMoYEHk
* https://www.youtube.com/watch?v=dm8eRdstBHY

## License

Apache License 2.0

## Author

Kukuh Tripamungkas Wicaksono

* GitHub: https://github.com/kukuhtw/ppv_stream_rust
* Email: kukuhtw@gmail.com
* LinkedIn: https://id.linkedin.com/in/kukuhtw
