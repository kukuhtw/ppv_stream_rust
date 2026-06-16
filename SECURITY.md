# Security Guide

This document explains the current security model of **PPV Stream**, what has already been hardened in the codebase, and what operators should do when deploying the application to production.

It is written for:

- developers maintaining the repository
- DevOps or platform engineers deploying the app
- reviewers performing security or compliance checks

---

## 1. Security Goals

The application protects:

- user accounts
- admin accounts
- creator content access
- wallet balances and wallet transactions
- fiat invoice state
- affiliate commission integrity
- payment webhooks
- streaming session authorization

The main goals are:

1. prevent unauthorized admin access
2. prevent forged purchases and payment state changes
3. prevent unauthorized video playback
4. prevent browser-based cross-site request abuse
5. reduce sensitive data exposure
6. keep webhook and replay handling idempotent

---

## 2. Authentication Model

### User authentication

- Users authenticate through `/auth/login`.
- Passwords are stored as Argon2 hashes.
- Successful login creates a signed session cookie named `ppv_session`.

### Admin authentication

- Admins authenticate through `/admin/login`.
- The session record stores whether the actor is admin.
- Sensitive `/admin/*` routes now require an active admin session in server-side checks.

### Session cookie protection

The session cookie is:

- `HttpOnly`
- `SameSite=Lax`
- `Secure` automatically when `BASE_URL` uses `https://`
- `Secure` can also be forced with `SESSION_COOKIE_SECURE=true`

Relevant code:

- [src/sessions.rs](src/sessions.rs)

---

## 3. Admin Route Protection

Sensitive admin endpoints must never rely only on frontend gating.

Server-side enforcement now exists for:

- admin data views
- payment monitoring
- payment settings
- SMTP settings
- wallet admin actions
- affiliate admin commission listing

Relevant files:

- [src/handlers/admin.rs](src/handlers/admin.rs)
- [src/handlers/affiliate.rs](src/handlers/affiliate.rs)

Operational requirement:

- Do not expose any additional admin routes without adding the same admin-session validation pattern.

---

## 4. Browser Request Protection

### Same-origin enforcement

State-changing browser requests that carry the `ppv_session` cookie are checked for same-origin behavior using `Origin` and `Referer`.

This helps block:

- classic CSRF attempts
- cross-site POST abuse from external pages

The middleware currently applies to cookie-authenticated mutating requests and excludes `/api/pay/*` webhook-related paths from the browser-origin rule.

Relevant file:

- [src/middleware.rs](src/middleware.rs)

### Rate limiting

Basic in-memory rate limiting is applied to sensitive auth-style endpoints such as:

- `/auth/login`
- `/admin/login`
- `/auth/register`
- `/auth/forgot`
- `/setup_admin`
- `/api/change_password`
- `/admin/change_password`

This is a first-line defense against brute-force and abuse.

Important limitation:

- It is process-local and in-memory, so it is not a distributed rate limiter.

For multi-instance deployments, use an external layer such as:

- Cloudflare
- Nginx rate limit
- load balancer rate limiting
- Redis-backed application rate limiting

---

## 5. Trusted Proxy Headers

Rate limiting and request fingerprinting can use:

- `X-Forwarded-For`
- `X-Real-IP`

But these headers are only trusted when:

```env
TRUST_PROXY_HEADERS=true
```

Default behavior:

- `TRUST_PROXY_HEADERS=false`

This is intentional, because trusting forwarded IP headers when the app is directly exposed can allow spoofing.

Enable `TRUST_PROXY_HEADERS=true` only when:

1. the app sits behind a trusted reverse proxy
2. the proxy strips or rewrites client forwarding headers correctly

Relevant files:

- [src/config.rs](src/config.rs)
- [src/middleware.rs](src/middleware.rs)

---

## 6. Payment Integrity

### Fiat payment invoice creation

The backend no longer trusts:

- client-supplied `user_id`
- client-supplied `amount_cents`

Instead:

- buyer identity is derived from the active session
- price is loaded from the server-side video record
- the owner cannot buy their own video

Relevant file:

- [src/handlers/payment_plugins.rs](src/handlers/payment_plugins.rs)

### Fiat webhook replay handling

Webhook replay is expected behavior from some providers.

The backend now short-circuits repeated successful webhook callbacks when an invoice is already marked paid, reducing duplicate side effects such as:

- repeated access grant attempts
- repeated disbursement attempts
- repeated affiliate processing

### x402 replay handling

x402 confirm now recognizes already-paid invoices and returns a replay-safe response instead of re-running the normal happy path.

Relevant files:

- [src/handlers/payment_plugins.rs](src/handlers/payment_plugins.rs)
- [src/handlers/pay.rs](src/handlers/pay.rs)

---

## 7. Admin Bootstrap Safety

The `/setup_admin` endpoint is intentionally restricted.

Current hardening:

- if `ADMIN_BOOTSTRAP_TOKEN` is missing, bootstrap is disabled
- if an admin account already exists, bootstrap is refused
- the route requires the correct bootstrap token in the query

Production guidance:

1. set a strong `ADMIN_BOOTSTRAP_TOKEN`
2. use bootstrap once
3. create the intended admin account
4. remove or rotate bootstrap secrets afterward

Relevant file:

- [src/handlers/setup.rs](src/handlers/setup.rs)

---

## 8. Data Exposure Controls

Public API responses should not expose unnecessary sensitive information.

Recent reductions include removing creator payout and contact fields from the public video listing response:

- email
- WhatsApp
- wallet address
- bank account

User lookup is also restricted:

- login is required
- email is no longer returned in lookup responses

Relevant file:

- [src/handlers/video.rs](src/handlers/video.rs)

---

## 9. Streaming Security

Playback is protected through:

- authenticated request to create a playback session
- access checks using ownership, purchase, allowlist, or admin role
- per-user playback session records
- HLS session ownership validation before serving segments
- path safety checks for HLS file access

Relevant file:

- [src/handlers/stream.rs](src/handlers/stream.rs)

Operational recommendation:

- keep `HLS_ROOT` on private server storage
- do not expose the raw session directory directly through a public web server bypassing the Rust app

---

## 10. Security Headers

The app now adds several response headers globally:

- `X-Frame-Options: DENY`
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: strict-origin-when-cross-origin`
- `Permissions-Policy: camera=(), microphone=(), geolocation=()`
- `Cross-Origin-Opener-Policy: same-origin`
- `Cross-Origin-Resource-Policy: same-origin`

Default `Cache-Control: no-store` is also added to dynamic responses unless another cache policy is already present.

Static asset paths are excluded from the blanket no-store default to avoid unnecessary frontend cache degradation.

Relevant file:

- [src/middleware.rs](src/middleware.rs)

---

## 11. Production Configuration Recommendations

Use at minimum:

```env
BASE_URL=https://stream.example.com
SESSION_COOKIE_SECURE=true
TRUST_PROXY_HEADERS=true
HMAC_SECRET=replace-with-a-long-random-secret
ADMIN_BOOTSTRAP_TOKEN=replace-with-a-long-random-token
RUST_LOG=info
```

If the app is directly exposed to the internet without a reverse proxy, use:

```env
TRUST_PROXY_HEADERS=false
```

---

## 12. Recommended Reverse Proxy Controls

Use Nginx, Caddy, Cloudflare, or another trusted edge layer to add:

- TLS termination
- request size limits
- additional rate limiting
- IP reputation / WAF rules
- bot filtering
- access logging

Suggested edge protections:

1. rate limit `/auth/login` and `/admin/login`
2. rate limit `/api/upload`
3. block obvious scanners on `/setup_admin`
4. enforce HTTPS redirect
5. log 4xx and 5xx responses

---

## 13. Remaining Security Work

The codebase is safer than before, but still has room for improvement.

Recommended next steps:

1. move from in-memory rate limiting to a shared store or edge-based limiter
2. expand audit logging further to additional security-sensitive business flows if needed
3. review legacy frontend routes and remove unused endpoints
4. add integration tests for:
   - admin auth enforcement
   - forged fiat purchase attempts
   - CSRF rejection
   - webhook replay handling
5. review upload and FFmpeg resource abuse limits under load
6. review outbound webhook and payment provider timeout/retry strategies

---

## 14. Reporting and Review Workflow

When changing security-sensitive code, review at least these files:

- `src/sessions.rs`
- `src/middleware.rs`
- `src/handlers/admin.rs`
- `src/handlers/payment_plugins.rs`
- `src/handlers/pay.rs`
- `src/handlers/setup.rs`
- `src/handlers/video.rs`
- `src/handlers/stream.rs`

If a feature touches payments, auth, storage, upload, or admin controls, it should be reviewed as a security-sensitive change even if the UI change looks small.
