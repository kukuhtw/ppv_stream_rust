# Federation Environment Variable Reference

All federation features are disabled by default and activated through environment
variables.  Variables marked **required when enabled** must be set before
starting the server if `FEDERATION_ENABLED=true`.

---

## Core federation

| Variable | Required | Default | Description |
|---|---|---|---|
| `FEDERATION_ENABLED` | — | `false` | Set to `1`, `true`, `yes`, or `on` to enable ActivityPub federation. All other federation variables are ignored when this is `false`. |
| `FEDERATION_BASE_URL` | required | value of `SERVER_BASE_URL` | Canonical HTTPS URL of this instance, e.g. `https://ppv.example.com`. Must use HTTPS except for `http://localhost` in development. Trailing slash is stripped automatically. |
| `FEDERATION_DOMAIN` | — | derived from `FEDERATION_BASE_URL` | Bare hostname used in `@context`, WebFinger, and actor URIs. Derived automatically from `FEDERATION_BASE_URL` when not set. Must contain only a hostname (no path, no `@`). |

---

## Security

| Variable | Required | Default | Description |
|---|---|---|---|
| `HMAC_SECRET` | required | — | Byte string used to encrypt actor RSA private keys at rest (HMAC-SHA256 envelope cipher). Must be kept secret and must not change after keys are generated. |
| `FEDERATION_ADMIN_TOKEN` | — | — | Bearer token required by the `X-Federation-Admin-Token` header on all `GET/POST/DELETE /api/federation/admin/*` endpoints. When unset, all admin endpoints return 403. |

---

## Delivery worker

The background delivery worker is started automatically when
`FEDERATION_ENABLED=true`.

| Variable | Required | Default | Description |
|---|---|---|---|
| `HMAC_SECRET` | required | — | Used by the delivery worker to decrypt actor private keys for HTTP Signature signing (shared with the security section above). |

Delivery retry schedule: exponential backoff `2^n` seconds plus 0–30 s jitter,
capped at 3 600 s (1 hour), for up to 10 attempts.

---

## Revenue sharing

| Variable | Required | Default | Description |
|---|---|---|---|
| *(none)* | — | — | Revenue share policies are configured at runtime via the admin API (`POST /api/federation/admin/revenue/policies`). No environment variables are needed beyond `FEDERATION_ADMIN_TOKEN`. |

---

## Quick-start checklist

```sh
# Minimum required for public federation
FEDERATION_ENABLED=true
FEDERATION_BASE_URL=https://ppv.example.com
HMAC_SECRET=<at-least-32-random-bytes>

# Recommended
FEDERATION_DOMAIN=ppv.example.com
FEDERATION_ADMIN_TOKEN=<strong-random-token>
```

---

## Notes

* **HTTP Signature max age**: inbound activities older than 30 seconds are
  rejected.  Ensure server clocks are synchronised (NTP).
* **SSRF protection**: all outbound federation HTTP requests enforce a
  10-second timeout, a 128 KB response limit, 3-redirect maximum, and
  reject responses from private/loopback/link-local IP ranges.
* **Payload limit**: inbound inbox payloads are capped at 64 KB.
* **Index-only constraint**: no remote video files, HLS manifests, or
  playback sessions are ever stored locally, regardless of configuration.
