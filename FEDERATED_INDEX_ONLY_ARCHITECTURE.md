# Federated Index-Only Architecture

## Status

Accepted architecture decision for the `add-feature/federated` branch.

This document is authoritative whenever another federation document appears to imply that remote video files, HLS segments, transcoded outputs, or protected media should be copied into the local server.

## Core Principle

Federation in PPV Stream Rust is an **index and discovery federation**, not a media replication network.

A PPV Stream instance may store and display public index information about videos hosted by another instance. It must not import, mirror, replicate, transcode, cache, or permanently store the remote video's media content.

The local server stores only enough information to help users discover the video and open the origin server.

## What the Local Server May Store

For a video hosted by another PPV Stream instance, the local server may store:

- Canonical video object URI
- Origin instance domain
- Origin creator actor URI
- Public title
- Public description or summary
- Public category
- Public content rating
- Public publication date
- Public price
- Public currency
- Public thumbnail URL
- Public trailer URL, when explicitly published by the origin
- Canonical watch page URL
- Canonical purchase or checkout URL
- Availability status
- Last metadata refresh timestamp
- Raw ActivityPub metadata for audit and compatibility purposes

A thumbnail should normally remain a remote URL. Local thumbnail proxying or caching is optional and must not be interpreted as permission to cache the video itself.

## What the Local Server Must Never Store for Remote Videos

The local server must not store or replicate:

- Original remote video files
- Remote MP4 files
- Remote HLS manifests for permanent reuse
- Remote HLS segments
- Remote DASH manifests or segments
- Transcoded copies
- Downloadable media copies
- Encryption keys
- DRM licenses
- Signed playback URLs
- Playback session tokens
- Viewer-specific watermark output
- Remote storage credentials
- Origin server object-storage paths
- Private previews
- Paid content fragments

The local server must not run FFmpeg against remote premium videos.

The local server must not copy remote premium media into local disk, S3, MinIO, Cloudflare R2, Backblaze B2, or another configured storage plugin.

## User Experience

A local user may browse a combined catalog containing:

- Local videos hosted by the current instance
- Remote video index entries discovered through federation

Every remote video entry must clearly show:

- The origin domain
- The remote creator identity
- A label such as `Remote video` or `Hosted on example.com`
- The canonical origin link

When the user selects a remote video, the local application must redirect or link the user to the origin server.

Recommended behavior:

```text
Local catalog
    -> Remote video index entry
    -> Open canonical origin page
    -> Origin server handles login, payment, authorization, and playback
```

The local server must not present itself as the host of the remote video.

## Payment Boundary

Payment for a remote video is handled by the origin server.

The local instance may display public price information received from the origin, but that information is informational and may become stale. The origin server remains authoritative for:

- Current price
- Currency
- Taxes
- Discounts
- Payment methods
- Payment confirmation
- Refunds
- Revenue distribution
- Affiliate commission
- Purchase receipts

The local server should redirect the user to the canonical origin checkout URL.

For the first implementation, the local instance must not create a local purchase record for a remote video and must not issue local playback access for that video.

## Playback Boundary

Playback always occurs from the origin server.

The origin server is responsible for:

- Authentication
- Purchase validation
- Allowlist validation
- Playback authorization
- HLS session creation
- Watermark generation
- Segment delivery
- Session expiration
- Access revocation

The local server does not proxy protected video traffic unless a future, separately reviewed architecture explicitly introduces a secure streaming proxy. Such a proxy is outside the current federation scope.

## ActivityPub Scope

ActivityPub is used to distribute public identity and index metadata.

Supported federation objects may include:

- Creator profile
- Public video index object
- Video publication activity
- Video metadata update activity
- Video deletion or unavailability activity
- Follow and unfollow activities
- Public announcement activities

ActivityPub payloads must contain canonical origin URLs rather than embedded protected media.

A federated video object should resemble:

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams",
    {
      "price": "https://ppv-stream.example/ns/price",
      "currency": "https://ppv-stream.example/ns/currency",
      "purchaseUrl": "https://ppv-stream.example/ns/purchaseUrl",
      "originHost": "https://ppv-stream.example/ns/originHost"
    }
  ],
  "id": "https://origin.example/federation/videos/123",
  "type": "Video",
  "attributedTo": "https://origin.example/users/alice",
  "name": "Rust Backend Masterclass",
  "summary": "Premium video course",
  "url": "https://origin.example/public/watch.html?video_id=123",
  "icon": {
    "type": "Image",
    "url": "https://origin.example/media/thumbnails/123.jpg"
  },
  "price": "5.00",
  "currency": "USD",
  "purchaseUrl": "https://origin.example/federation/checkout/123",
  "originHost": "origin.example",
  "published": "2026-06-20T10:00:00Z"
}
```

The object must not include:

- Direct premium MP4 URL
- Direct HLS manifest URL
- Signed playback URL
- Storage object key
- Private preview URL
- DRM secret

## Database Model

The remote catalog is an index table, not a media table.

Recommended schema:

```sql
CREATE TABLE remote_video_catalog (
    id UUID PRIMARY KEY,
    object_uri TEXT NOT NULL UNIQUE,
    origin_actor_uri TEXT NOT NULL,
    origin_domain TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    thumbnail_url TEXT,
    trailer_url TEXT,
    canonical_url TEXT NOT NULL,
    checkout_url TEXT,
    price_amount NUMERIC,
    price_currency TEXT,
    content_rating TEXT,
    availability_status TEXT NOT NULL DEFAULT 'available',
    published_at TIMESTAMPTZ,
    raw_object JSONB NOT NULL,
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

The table must not contain:

- Local media filename
- Local HLS path
- Local object-storage key
- Local transcoding state
- Local playback session reference
- Local watermark path

## Required Application Changes

### Catalog Query

The catalog service should combine two sources:

```text
Local videos table
Remote video catalog table
```

Each response item must include a hosting indicator:

```json
{
  "hosting_type": "remote",
  "origin_domain": "origin.example",
  "canonical_url": "https://origin.example/public/watch.html?video_id=123"
}
```

Allowed values:

- `local`
- `remote`

### Remote Video Detail

A remote video detail page may show indexed metadata, but its main action must be:

```text
View on origin server
```

or:

```text
Buy and watch on origin server
```

### Upload and Worker Isolation

The upload handler, FFmpeg worker, storage migration worker, and HLS worker must only process locally owned videos.

Add explicit guards such as:

```text
hosting_type must equal local
```

Remote catalog records must never enter the local upload, transcoding, storage migration, or HLS processing queue.

## API Requirements

Suggested read endpoints:

```http
GET /api/videos
GET /api/videos/:id
GET /api/remote-videos/:id
GET /api/remote-creators/:id
```

A remote catalog response should contain metadata and canonical links only.

It must not return a local playback endpoint for a remote video.

The following local endpoints must reject remote catalog identifiers:

```http
POST /api/wallet/pay
GET /api/request_play
POST /api/allow
POST /api/video_update
```

Recommended error:

```json
{
  "error": "REMOTE_VIDEO_ORIGIN_REQUIRED",
  "message": "This video is hosted by another instance. Continue on the origin server.",
  "origin_url": "https://origin.example/public/watch.html?video_id=123"
}
```

## Synchronization Rules

The local server may refresh public metadata from the canonical ActivityPub object.

Synchronization rules:

1. The origin object URI is the primary identity.
2. The origin server is authoritative.
3. Local edits to remote metadata are not allowed.
4. A remote `Update` replaces only permitted public index fields.
5. A remote `Delete` marks the index entry unavailable or deleted.
6. Missing remote content does not trigger media recovery or replication.
7. If the origin is unreachable, the local index may remain visible with an `Origin temporarily unavailable` label.
8. Stale prices must be labeled as informational until confirmed on the origin server.

## Security Rules

The implementation must prevent remote metadata from becoming a path to media replication or server-side request abuse.

Required protections:

- Verify ActivityPub HTTP Signatures
- Validate canonical URLs
- Block private and local network destinations
- Limit redirect chains
- Limit metadata response size
- Sanitize remote HTML
- Restrict accepted URL schemes to HTTPS in production
- Never automatically follow media URLs from remote metadata
- Never download video attachments from remote ActivityPub objects
- Never send storage credentials to a remote instance
- Never expose local playback tokens through federation

## Revised MVP Scope

The index-only federation MVP should include:

1. Federation feature flag
2. WebFinger discovery
3. ActivityPub actor endpoints
4. HTTP Signature signing and verification
5. Follow, Accept, Reject, and Undo
6. Public video index object
7. Create, Update, and Delete metadata activities
8. Remote creator cache
9. Remote video index table
10. Combined local and remote catalog
11. Origin labels and canonical links
12. Domain moderation rules
13. Delivery queue and retry worker
14. Two-instance metadata synchronization tests

The MVP explicitly excludes:

- Remote video file transfer
- Remote video caching
- Remote transcoding
- Remote HLS hosting
- Local playback of remote video
- Local payment for remote video
- Local entitlement for remote video
- Cross-instance wallet settlement

## Acceptance Criteria

The index-only federation implementation is complete when:

- A user can discover a creator on another instance
- Public remote video metadata appears in the local catalog
- Every remote result displays its origin instance
- Selecting a remote result opens the canonical origin page
- No remote video file is copied into local storage
- No remote HLS segment is stored locally
- No remote video is submitted to FFmpeg
- No local playback session is generated for a remote video
- No local wallet payment is accepted for a remote video
- Metadata updates propagate from the origin instance
- Metadata deletion propagates from the origin instance
- Blocked instances disappear from the local federated catalog
- Automated tests verify that remote media URLs are never downloaded

## Final Rule

> PPV Stream federation shares only public identity and video index metadata. The actual video content remains exclusively on the origin server, and users must access, purchase, and watch it through that origin server.
