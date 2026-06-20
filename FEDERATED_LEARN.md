# Federated Learning Guide

## 1. What Federation Means in This Project

In PPV Stream, federation means that multiple independent servers can recognize each other, exchange public information, and let users discover creators across instances.

This is not full data sharing. It is a controlled federation model.

The guiding idea is:

- Public identity can move across servers.
- Public video index metadata can move across servers.
- Payment, entitlement, playback, and media delivery stay under the origin server.

That design gives the platform a network effect without giving away ownership of the video, the buyer access policy, or the payment records.

## 2. Why Federation Exists

Federation helps the platform grow beyond one isolated site.

It allows:

- A creator on one instance to be discovered by users on another instance.
- A buyer to see public video metadata before visiting the origin server.
- A community to grow across organizations, brands, or partner networks.
- A platform operator to stay in control of content, pricing, and disbursement.

For PPV Stream, federation is a distribution layer, not a media replication layer.

## 3. The Core Rule

The most important rule is:

> Federation shares public identity and index metadata. The origin server keeps the actual media, access control, payment, watermarking, and playback session logic.

If a remote instance can see a video, it should still redirect the user back to the origin server for purchase and playback.

## 4. What Can Be Federated

The following information is safe to federate:

- Creator username and public profile
- Actor URI and profile URL
- Public video title and description
- Thumbnail or preview image URL
- Price and currency
- Publication status
- Public comments and public social activity
- Follow and unfollow actions
- Moderation status for public federation rules

This gives remote instances enough context to act as discovery hubs.

## 5. What Must Stay Local

The following must remain on the origin server:

- Password hashes
- Session cookies
- Email addresses unless intentionally public
- Payment secrets and webhook secrets
- Wallet balances
- Internal transaction history
- Purchase entitlement records
- Playback sessions
- Signed HLS URLs
- Watermark output
- Media files and HLS segments

That separation protects the platform and keeps the monetization model authoritative.

## 6. How Federation Works Step by Step

### Step 1: Discovery

A remote server resolves a creator using WebFinger, for example:

```text
acct:alice@example.com
```

The result points to the creator actor URI on the origin server.

### Step 2: Actor Fetch

The remote server fetches the actor document.

That actor document exposes only public identity data and a public key.

### Step 3: Follow

If a remote user wants updates from that creator, they send a `Follow` activity.

The origin server decides whether to accept or reject it.

### Step 4: Publish Public Video Metadata

When a creator publishes a public video, the origin server can broadcast a `Create` activity with index-only metadata.

Remote instances store the public metadata in their local catalog.

### Step 5: View and Purchase

When a remote user clicks a video, the remote instance should send them to the origin server.

The origin server handles:

- checkout
- payment confirmation
- entitlement creation
- access grant
- playback authorization

### Step 6: Playback

Playback always happens from the origin.

This is important because the origin server controls the HLS session, watermarking, and revocation.

## 7. Main Federation Roles

### Origin Server

The origin server owns the video and the buyer relationship.

It is responsible for:

- hosting media
- setting prices
- confirming payments
- issuing access
- revoking access
- tracking disbursement

### Remote Instance

The remote instance acts as a discovery and community front door.

It can:

- show remote public profiles
- show remote public videos
- collect follows
- route users to the origin
- store public index metadata

### Buyer

The buyer may start on a remote instance, but they complete the purchase on the origin server.

### Creator

The creator owns the content and receives revenue according to the configured payout rules.

## 8. Index-Only Federation

This repository uses index-only federation.

That means the remote instance stores only enough information to:

- render a catalog
- show the creator
- show the price
- link back to the origin

It does not:

- mirror media
- transcode remote uploads
- create remote playback sessions
- store signed playback tokens
- act as a proxy for paid content

This keeps federation lightweight and safer to operate.

## 9. Revenue and Settlement

Federation does not remove the business model.

It changes where discovery happens, not who owns the sale.

The origin server still:

- calculates payment amounts
- records purchases
- applies affiliate logic
- creates disbursement liabilities
- pays creators and partners

If a federated buyer comes from another instance, the sale is still attributed and settled at the origin.

## 10. Security Mindset

Federation expands the trust boundary, so it needs strong validation.

Important checks include:

- HTTP signature verification
- digest validation
- replay protection
- SSRF protection
- domain moderation
- size limits on inbound payloads
- careful handling of remote HTML

The goal is to accept only the public data that belongs in federation.

## 11. Mental Model for New Contributors

If you are new to the codebase, keep this mental model in mind:

1. Remote instances can discover the creator.
2. Remote instances can cache public metadata.
3. Remote instances cannot own the sale.
4. The origin instance owns payment and playback.
5. The user always returns to the origin for the paid path.

If a proposed change breaks that model, it is probably outside the intended federation scope.

## 12. Good Places to Read Next

- [FEDERATED_INDEX_ONLY_ARCHITECTURE.md](FEDERATED_INDEX_ONLY_ARCHITECTURE.md)
- [FEDERATED_IMPLEMENTATION.md](FEDERATED_IMPLEMENTATION.md)
- [FEDERATED_REVENUE_SHARING.md](FEDERATED_REVENUE_SHARING.md)
- [DATA_FLOW.md](DATA_FLOW.md)
- [PAYMENT.md](PAYMENT.md)
- [AFFILIATE.md](AFFILIATE.md)

## 13. Final Summary

Federation in PPV Stream is a way to share discovery, not control.

The remote server can help people find content, but the origin server remains the source of truth for payment, access, media delivery, and business records.
