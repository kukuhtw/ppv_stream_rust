# Federated Implementation Tasks

## Scope Rule

Federation is index-only.

Remote PPV Stream instances may exchange public creator identity and public video index metadata. Remote video files, HLS manifests, HLS segments, transcoded outputs, playback sessions, and protected media must remain on the origin server.

## Execution Status Legend

- `[ ]` Not started
- `[~]` In progress
- `[x]` Completed
- `[!]` Blocked or requires design approval

## Phase 0: Foundation

- [x] Create branch `add-feature/federated`
- [x] Add federation implementation documentation
- [x] Add index-only architecture decision
- [x] Add federated revenue-sharing documentation
- [x] Create implementation task checklist
- [x] Add dedicated federation configuration
- [x] Add configuration validation
- [x] Add federation database migration to runtime `sql` directory
- [x] Add federation module skeleton
- [x] Add federation feature flag to application routing
- [x] Add unit tests for configuration defaults and HTTPS validation

## Phase 1: Discovery

- [x] Implement `GET /.well-known/webfinger`
- [x] Implement `GET /.well-known/nodeinfo`
- [x] Implement `GET /nodeinfo/2.1`
- [x] Implement local ActivityPub actor endpoint
- [x] Return ActivityPub content type
- [x] Add discovery endpoint unit tests
- [x] Add disabled-federation configuration test

## Phase 2: Remote Index Storage

- [x] Create `federation_instances`
- [x] Create `federation_actors`
- [x] Create `federation_activities`
- [x] Create `federation_delivery_jobs`
- [x] Create `remote_video_catalog`
- [x] Create `federation_domain_rules`
- [x] Add indexes and constraints
- [x] Confirm remote catalog has no media storage columns
- [ ] Add migration rollback documentation

## Phase 3: ActivityPub Identity and Security

- [x] Generate local actor keys
- [x] Encrypt actor private keys at rest
- [x] Implement HTTP Signature creation
- [x] Implement HTTP Signature verification
- [x] Implement Digest verification
- [x] Add replay protection
- [x] Add maximum request age validation
- [x] Add federation payload limits
- [x] Add SSRF-safe remote resolver
- [x] Block private and local address ranges

## Phase 4: Follow Federation

- [x] Implement actor inbox
- [x] Implement shared inbox
- [x] Implement actor outbox
- [x] Implement followers collection
- [x] Implement following collection
- [x] Implement `Follow`
- [x] Implement `Accept`
- [x] Implement `Reject`
- [x] Implement `Undo`
- [x] Add activity deduplication
- [x] Add delivery queue worker
- [x] Add exponential retry with jitter

## Phase 5: Video Index Federation

- [x] Define public ActivityPub video index object
- [x] Publish `Create` when a local video becomes public
- [x] Publish `Update` when public metadata changes
- [x] Publish `Delete` when a video is removed or unavailable
- [x] Process remote video `Create`
- [x] Process remote video `Update`
- [x] Process remote video `Delete`
- [x] Build combined local and remote catalog query
- [x] Add `hosting_type` to catalog responses
- [x] Display origin instance for remote videos
- [x] Add canonical origin watch and checkout links
- [x] Reject local payment requests for remote videos
- [x] Reject local playback requests for remote videos
- [x] Prevent remote records from entering upload workers
- [x] Prevent remote records from entering FFmpeg workers
- [x] Prevent remote records from entering storage migration workers

## Phase 6: Moderation and Administration

- [x] Add domain allow rule
- [x] Add domain silence rule
- [x] Add domain media-rejection rule
- [x] Add domain suspension rule
- [x] Add domain block rule
- [x] Add federation overview endpoint
- [x] Add known-instances endpoint
- [x] Add activity log endpoint
- [x] Add delivery queue endpoint
- [x] Add failed-delivery retry action
- [x] Add cached remote-content removal action

## Phase 7: Provider Referral and Revenue Sharing

- [x] Create `federation_referrals`
- [x] Create `revenue_share_policies`
- [x] Create `federation_revenue_shares`
- [x] Create `revenue_ledger_entries`
- [x] Implement signed traffic-provider referral payload
- [x] Implement remote affiliate attribution
- [x] Capture attribution at invoice creation
- [x] Snapshot revenue policy at invoice creation
- [x] Calculate revenue in integer minor units
- [x] Add basis-point validation
- [x] Add idempotent revenue processing
- [x] Add refund reversal entries
- [x] Add chargeback reversal entries
- [x] Add provider settlement reporting
- [ ] Add affiliate settlement reporting
- [ ] Add optional X402 direct split

## Phase 8: Testing

- [x] Unit test WebFinger parsing
- [x] Unit test NodeInfo response
- [x] Unit test actor serialization
- [x] Unit test ActivityPub video serialization
- [x] Unit test signature generation
- [x] Unit test signature verification
- [x] Unit test SSRF protections
- [x] Unit test referral verification
- [x] Unit test revenue calculations
- [ ] Integration test two PPV Stream instances
- [x] Verify no remote media file is downloaded
- [x] Verify no remote HLS segment is stored
- [x] Verify no local playback session is created for remote video
- [x] Verify no local payment is accepted for remote video
- [x] Verify blocked instances are excluded
- [x] Verify duplicate activities are idempotent

## Phase 9: Documentation and Operations

- [ ] Add environment variable reference
- [ ] Add federation setup guide
- [ ] Add instance administration guide
- [ ] Add moderation guide
- [ ] Add troubleshooting guide
- [ ] Add provider settlement guide
- [ ] Add privacy documentation
- [ ] Add security threat model
- [ ] Add monitoring metrics documentation
- [ ] Add backup and key-rotation guide

## Current Implementation Batch

Completed:

1. Federation configuration
2. Configuration validation
3. Runtime database migration
4. Federation module skeleton
5. WebFinger endpoint
6. NodeInfo endpoints
7. Local actor endpoint
8. Basic configuration tests
9. Application router integration

Next tasks:

1. Phase 7: revenue sharing tables and referral payload
2. Phase 8: integration tests and safety verification

## First Batch Result

- Federation is disabled by default
- Federation can be enabled using environment variables
- Public federation requires HTTPS, except localhost development
- The local domain and base URL are validated
- Database tables for index-only federation are registered in runtime migrations
- WebFinger can resolve a discoverable local user
- NodeInfo reports `index-only` federation mode
- A local actor endpoint returns ActivityPub JSON
- No remote media download or playback logic has been introduced

## Third Batch Result

- ActivityPub Video object builder with PPVStream extension namespace (`video_index.rs`)
- `publish_create` / `publish_update` / `publish_delete` with follower broadcast via deduped shared inboxes
- `process_remote_create` / `process_remote_update` / `process_remote_delete` → upserts `remote_video_catalog`
- Inbound Create/Update/Delete activity dispatch added to `activities.rs`
- Combined local+remote catalog endpoint `GET /api/federation/catalog` with `hosting_type` field
- ActivityPub Video object serving at `GET /videos/:id` with Accept header negotiation
- Canonical origin `watch_url` and `checkout_url` included in remote catalog entries
- Remote media structurally separated: remote catalog has no `filename`, `hls_master` — cannot be played locally

## Second Batch Result

- RSA 2048-bit actor key generation (`src/federation/keys.rs`)
- HMAC-SHA256 envelope encryption for private keys at rest
- HTTP Signature creation and verification with RSA-SHA256 (`src/federation/signatures.rs`)
- SHA-256 Digest header creation and verification
- Maximum request age enforcement (30 seconds clock skew tolerance)
- SSRF-safe remote actor resolver (`src/federation/resolver.rs`)
- Private and reserved IP range blocking for outbound federation requests
- Actor inbox endpoint with signature verification and payload size limit (`src/federation/collections.rs`)
- Shared inbox endpoint
- Outbox, followers, and following collection endpoints
- Activity deduplication via `activity_uri` uniqueness check
- Local actor document now includes `publicKey` block and Security context
- Discovery endpoint unit tests added to `mod.rs`
- Signature round-trip tests in `signatures.rs`
- Key generation and encrypt/decrypt tests in `keys.rs`
- SSRF protection tests in `resolver.rs`
