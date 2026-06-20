# Federation Security Threat Model

## Trust boundary

PPV Stream uses ActivityPub for index-only federation.  The security
boundary is: **no remote instance can cause local media to be stored,
played, or paid for.**

---

## Threat catalogue

### T1 — Forged activity injection

**Threat**: attacker submits fake activities (e.g. fake `Follow`, fake
`Create`) to the inbox without a valid HTTP Signature.

**Mitigations**:
* HTTP Signature verification (RSA-SHA256) on every inbound inbox POST.
* Actor public keys are fetched fresh from the actor's home instance via
  SSRF-safe DNS resolution.
* `Digest: SHA-256=<hash>` header is verified when present.
* Activities older than 30 seconds are rejected.

---

### T2 — Replay attack

**Threat**: attacker replays a previously captured, legitimately signed
activity to trigger duplicate processing (e.g. double-follow).

**Mitigations**:
* `activity_uri` uniqueness constraint in `federation_activities`.
* Already-seen activity URIs are acknowledged (HTTP 202) but not
  reprocessed.
* Referral tokens include a random nonce and a 24-hour expiry.

---

### T3 — SSRF via remote actor URL

**Threat**: malicious actor document contains an `inbox` URL pointing to
an internal network address, causing the delivery worker to make requests
to internal services.

**Mitigations**:
* All outbound federation requests go through `assert_safe_url`, which:
  * Requires HTTPS.
  * Resolves DNS and blocks private/loopback/link-local/multicast ranges
    (10.x.x.x, 192.168.x.x, 172.16–31.x.x, 127.0.0.1/::1, 169.254.x.x,
    fc00::/7, multicast).
  * Limits response size to 128 KB.
  * Limits redirects to 3.
  * Enforces a 10-second connection timeout.

---

### T4 — Private key exfiltration

**Threat**: attacker reads the database and obtains actor RSA private keys.

**Mitigations**:
* Private keys are stored as `HMAC-SHA256 XOR stream cipher` ciphertext
  keyed by `HMAC_SECRET` (format: `v1:<b64-nonce>:<b64-ciphertext>`).
* `HMAC_SECRET` must be supplied only via environment variable (not in DB).
* A database dump without `HMAC_SECRET` yields no usable private keys.

---

### T5 — Domain spoofing in referral tokens

**Threat**: attacker forges a referral token claiming to be from
`trusted.example` to earn undeserved revenue share.

**Mitigations**:
* Referral tokens are signed with the referring actor's RSA private key.
* Signature is verified against the public key fetched (SSRF-safely) from
  the actor's home instance.
* Tokens expire after 24 hours.
* Nonce prevents reuse.

---

### T6 — Inbox payload bomb

**Threat**: attacker sends extremely large payloads to the inbox to
exhaust memory.

**Mitigation**:
* Inbound inbox payloads are capped at 64 KB.
* Requests exceeding the limit receive HTTP 413.

---

### T7 — Delivery queue DoS

**Threat**: attacker crafts a scenario that floods the delivery queue with
unreachable targets, preventing legitimate deliveries.

**Mitigations**:
* Maximum 10 retry attempts per job.
* Exponential backoff (2^n seconds + 0–30 s jitter, capped at 1 hour).
* Domain `block`/`suspend` rules stop delivery immediately without burning
  retry budget.
* Batch size limited to 10 jobs per 30-second cycle.

---

### T8 — Content injection via remote metadata

**Threat**: malicious video metadata contains XSS payloads or misleading
content in title/description.

**Mitigation**:
* All remote metadata is stored as plain text and must be HTML-escaped by
  the frontend before rendering.
* The `raw_object` JSONB column stores the full AP object for audit but
  is not rendered directly.

---

## Security properties guaranteed by architecture

* Remote video files are never downloaded or stored.
* Remote HLS manifests and segments are never stored.
* No local playback sessions are created for remote videos (enforced in
  `request_play` handler).
* No local payments are accepted for remote videos (enforced in
  `pay_options`, `x402_start`, and `all_options` handlers).
* Worker queues (upload, FFmpeg, storage migration) only process jobs
  explicitly enqueued from local uploads; `remote_video_catalog` entries
  have no file paths and cannot be enqueued.

---

## Out of scope

* DDoS protection (handled at the reverse proxy / CDN layer).
* TLS certificate management.
* Database-level access control.
* Admin API authentication beyond the bearer token (consider IP allowlist
  at the reverse proxy for additional protection).
