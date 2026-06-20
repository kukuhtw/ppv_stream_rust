# Federation Privacy Documentation

## What information is shared

### Outbound (data this instance sends to remote instances)

When a local creator enables federation for a video, the following
metadata is broadcast as an ActivityPub `Create` activity:

* Video title
* Video description
* Price (as a decimal string in USD)
* Canonical watch URL (`/watch/<id>` on this instance)
* Checkout URL (`/checkout/<id>` on this instance)
* Creator actor URI
* Object URI (`/videos/<id>` on this instance)
* Publication timestamp

**Never sent**:
* Video file bytes, HLS manifests, or segments
* Creator email address, password, or wallet private key
* Buyer identities or purchase records
* Internal video processing state

### Inbound (data received from remote instances)

When a remote instance publishes a video, this instance caches:

* Video metadata (title, description, price, origin URLs)
* Origin domain and actor URI

### Actor documents

Actor documents served at `GET /users/:username` include:

* Username (preferred username)
* Public RSA key (PEM, for HTTP Signature verification)
* Inbox, outbox, followers, and following URLs
* Profile summary (if provided)
* Federation mode note ("Index only. Remote video media is never replicated.")

**Not included**: email, wallet address, purchase history, session tokens.

### HTTP Signatures

Every outbound delivery request includes:
* `Date` header
* `Digest` header (SHA-256 of the body)
* `Signature` header (RSA-SHA256, key ID, signed headers)

These headers are used by the recipient to verify authenticity.  They
do not carry personal data beyond the actor's key ID.

---

## Data retention

| Data | Retention |
|---|---|
| Remote actor public keys | Until the actor is deleted or the domain is purged. |
| Remote video catalog entries | Until the origin sends a `Delete` activity or an admin purges the domain. |
| Federation activities (inbound/outbound) | Stored indefinitely; no automatic expiry (add a cron job to prune old records as needed). |
| Delivery jobs | Stored until delivered, failed, or cancelled. |
| Revenue ledger entries | Immutable; never deleted (financial audit trail). |

---

## Viewer privacy

* Viewer identities are **not shared** with remote instances during catalog browsing.
* Referral tokens use an opaque `nonce` field; they do not contain viewer
  identifiers.
* If a viewer clicks through to a remote instance (via `watch_url` or
  `checkout_url`), the remote instance's own privacy policy applies from
  that point.

---

## Data minimisation

This federation implementation is index-only:

* No remote video files are ever downloaded or stored.
* No remote HLS segments or manifests are stored.
* No local playback sessions are created for remote videos.
* No local payments are accepted for remote videos.

The only data retained about remote content is the public video metadata
needed to display it in the federated catalog.
