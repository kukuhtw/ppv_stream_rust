# Federation Setup Guide

## Prerequisites

* A running PPV Stream instance reachable over HTTPS on a public domain.
* TLS certificate for your domain (e.g., via Let's Encrypt).
* PostgreSQL 14+ (migrations run automatically on startup).
* Synchronized system clock (NTP).  HTTP Signatures are rejected if the
  request timestamp is more than 30 seconds old.

---

## 1. Set environment variables

Add the following to your `.env` or container environment:

```sh
FEDERATION_ENABLED=true
FEDERATION_BASE_URL=https://ppv.example.com
HMAC_SECRET=<at-least-32-random-bytes>
FEDERATION_ADMIN_TOKEN=<strong-random-token>
```

Generate safe random values with:

```sh
openssl rand -hex 32   # for HMAC_SECRET
openssl rand -hex 24   # for FEDERATION_ADMIN_TOKEN
```

## 2. Start the server

The database migrations for federation (`040`, `041`) run automatically on
startup.  The delivery worker is also started automatically when
`FEDERATION_ENABLED=true`.

```sh
cargo run --release
# or via Docker Compose
docker compose up
```

## 3. Verify discovery endpoints

```sh
# WebFinger
curl -s "https://ppv.example.com/.well-known/webfinger?resource=acct:alice@ppv.example.com" | jq .

# NodeInfo
curl -s "https://ppv.example.com/.well-known/nodeinfo" | jq .

# Actor document
curl -s -H "Accept: application/activity+json" \
     "https://ppv.example.com/users/alice" | jq .
```

## 4. Enable a creator for federation

By default all users have `federation_enabled = TRUE` and `discoverable = TRUE`.
If you want only specific creators to be discoverable:

```sql
-- Disable federation for a specific user
UPDATE users SET federation_enabled = FALSE WHERE username = 'alice';

-- Or disable discovery (actor exists but is not in WebFinger)
UPDATE users SET discoverable = FALSE WHERE username = 'alice';
```

## 5. Generate actor keys

Actor RSA keys are generated lazily on first use by calling
`ensure_local_actor_keys`.  You can pre-generate them for all existing
creators:

```sql
-- Keys are inserted by ensure_local_actor_keys() called from the
-- federation key setup flow.  Run this query to check who has keys:
SELECT u.username, fa.id IS NOT NULL AS has_key
FROM users u
LEFT JOIN federation_actors fa ON fa.local_user_id = u.id AND fa.is_local = TRUE
WHERE u.federation_enabled = TRUE;
```

## 6. Publish a video for federation

In the video management interface (or via API):

```sh
curl -X POST https://ppv.example.com/api/videos/update \
  -H "Cookie: session=..." \
  -d "id=<video_id>&title=My+Video&description=...&price_cents=999&federation_visibility=public"
```

Setting `federation_visibility=public` broadcasts a `Create` activity to
all followers of the video owner.

## 7. Check the admin overview

```sh
curl -s \
  -H "X-Federation-Admin-Token: <your-token>" \
  "https://ppv.example.com/api/federation/admin/overview" | jq .
```

---

## Development / localhost

For local testing without HTTPS:

```sh
FEDERATION_ENABLED=true
FEDERATION_BASE_URL=http://localhost:8080
HMAC_SECRET=dev-secret-do-not-use-in-production
```

`http://localhost` URLs bypass the HTTPS requirement in config validation.
