# Admin Authentication and Password Management

→ [README.md](README.md) | [WALLET.md](WALLET.md) | [AFFILIATE.md](AFFILIATE.md) | [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md)

## Overview

The platform uses database-backed authentication for administrator accounts.

Administrator credentials are not validated directly against environment variables during normal login operations.

Authentication is performed using the `users` table and Argon2 password hashes stored in the database.

## Admin Login

Login page:

```text
/public/admin/login.html
```

Login endpoint:

```text
POST /admin/login
```

After successful authentication the administrator is redirected to:

```text
/public/admin/dashboard.html
```

## Password Storage

Passwords are stored in PostgreSQL.

Table:

```text
users
```

Columns:

```text
email
password_hash
is_admin
```

Passwords are hashed using Argon2.

## Authentication Flow

1. User submits email and password.
2. System loads the user record from the database.
3. Argon2 verifies the password against `password_hash`.
4. System verifies `is_admin = true`.
5. A signed session cookie is created.
6. Administrator gains access to the dashboard.

## Bootstrap Administrator

Bootstrap endpoint:

```text
GET /setup_admin
```

Environment variables:

```text
ADMIN_BOOTSTRAP_EMAIL
ADMIN_BOOTSTRAP_PASSWORD
ADMIN_BOOTSTRAP_TOKEN
```

These values are only used for initial administrator creation, promotion, or emergency password reset.

## Change Password

Endpoint:

```text
POST /admin/change_password
```

The application:

1. Verifies the current password.
2. Creates a new Argon2 hash.
3. Updates `users.password_hash`.

Database update:

```sql
UPDATE users
SET password_hash = ?
WHERE id = ?
```

## Does Password Change Update .env?

No.

Changing an administrator password updates only the database.

The `.env` file is never modified by the application.

## Source of Truth

Administrator authentication source:

```text
Database (users.password_hash)
```

Not:

```text
.env
```

---

## Admin Wallet Management

Admin wallet endpoints are defined in `src/handlers/admin.rs` and use `AdminState { pool }`.

| Route | Action |
|-------|--------|
| `GET /admin/wallet/transactions?txn_type=&status=&limit=` | List all wallet transactions with filter |
| `POST /admin/wallet/transactions/:id/approve` | Approve a deposit → credit user balance |
| `POST /admin/wallet/transactions/:id/complete` | Mark a withdrawal as paid (balance already held) |
| `POST /admin/wallet/transactions/:id/reject` | Reject any pending transaction; refunds balance if withdrawal |

All actions accept an optional `{ "admin_note": "..." }` body for audit trail.

The admin UI is at `/public/admin/wallet.html`.

→ Full wallet documentation: [WALLET.md](WALLET.md)

---

## Admin Affiliate Commissions

The affiliate admin endpoint is defined in `src/handlers/affiliate.rs`.

| Route | Action |
|-------|--------|
| `GET /admin/affiliate/commissions?limit=100` | All commissions across the platform with totals |

Response includes affiliate username, buyer username, creator username, video title, commission amount, payment method, and timestamp.

→ Full affiliate documentation: [AFFILIATE.md](AFFILIATE.md)

---

## Admin Payment Monitoring

| Route | Action |
|-------|--------|
| `GET /admin/payments?provider=&status=&limit=` | Fiat invoices with filter |
| `POST /admin/payments/:uid/disburse` | Trigger creator payout (Xendit: real API; others: mark manual) |
| `GET /admin/data` | Raw records: users, sessions, videos, purchases, allowlists |
| `GET/POST /admin/smtp` | Email server configuration |

---

## Related Documentation

- [README.md](README.md) — platform overview
- [WALLET.md](WALLET.md) — wallet system including admin deposit/withdrawal flows
- [AFFILIATE.md](AFFILIATE.md) — affiliate commission system
- [PAYMENT.md](PAYMENT.md) — all payment methods
- [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md) — code reference for admin handlers
