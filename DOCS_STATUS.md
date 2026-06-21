# Documentation Status

Last verified against the repository source code on 20 June 2026.

## Review scope

The documentation was checked against the current Axum routes, Cargo metadata, enabled modules, payment plugins, storage plugins, wallet handlers, affiliate handlers, chat handlers, and optional federation behavior.

## Documentation map

| Area | Source | Documentation |
| --- | --- | --- |
| Authentication | `src/handlers/auth_user.rs`, `src/handlers/auth_admin.rs`, `src/sessions.rs` | `ADMIN_AUTHENTICATION.md`, `SECURITY.md` |
| Upload and processing | `src/handlers/upload.rs`, `src/worker.rs`, `src/ffmpeg.rs` | `README.md`, `TECHNICAL_DOCUMENTATION.md` |
| Playback | `src/handlers/stream.rs` | `README.md`, `DATA_FLOW.md` |
| Payments | `src/handlers/pay.rs`, `src/handlers/payment_plugins.rs` | `PAYMENT.md`, `PAYMENT_PLUGIN_ARCHITECTURE.md` |
| Wallet | `src/handlers/wallet.rs` | `WALLET.md` |
| Affiliate | `src/handlers/affiliate.rs`, `src/commission.rs` | `AFFILIATE.md` |
| Storage | `src/plugins/storage`, `src/storage_settings.rs` | `STORAGE_MIGRATION.md`, `STORAGE_ADMIN_MOCKUP.md` |
| Chat | `src/handlers/chat.rs` | `README.md`, `DATA_FLOW.md` |
| Federation | `src/federation` | `FEDERATED_LEARN.md`, `README.md` |
| Deployment | `Dockerfile`, `docker-compose.yml`, `Makefile` | `SETUP.md`, `DEPLOYMENT.md` |

## Accuracy rules

1. Source code and migrations are authoritative when documentation conflicts with implementation.
2. A provider listed in the repository is not automatically active. Runtime configuration determines availability.
3. Federation and the X402 watcher are optional.
4. Watermarking and session scoped HLS discourage copying but do not make copying impossible.
5. The internal wallet is an application ledger, not a bank account or blockchain wallet.
6. Production use still requires infrastructure, security, monitoring, backup, and legal review.
7. Example accounts are for development only.

## Maintenance checklist

1. Update the detailed document when a feature changes.
2. Update the README when user visible behavior changes.
3. Update `updated.md` when a feature or migration is added.
4. Check links, code paths, route names, and diagrams.
5. Mark optional and experimental behavior clearly.
