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
- [ ] Implement `Reject`
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
- [ ] Prevent remote records from entering upload workers
- [ ] Prevent remote records from entering FFmpeg workers
- [ ] Prevent remote records from entering storage migration workers

## Phase 6: Moderation and Administration

- [ ] Add domain allow rule
- [ ] Add domain silence rule
- [ ] Add domain media-rejection rule
- [ ] Add domain suspension rule
- [ ] Add domain block rule
- [ ] Add federation overview endpoint
- [ ] Add known-instances endpoint
- [ ] Add activity log endpoint
- [ ] Add delivery queue endpoint
- [ ] Add failed-delivery retry action
- [ ] Add cached remote-content removal action

## Phase 7: Provider Referral and Revenue Sharing

- [ ] Create `federation_referrals`
- [ ] Create `revenue_share_policies`
- [ ] Create `federation_revenue_shares`
- [ ] Create `revenue_ledger_entries`
- [ ] Implement signed traffic-provider referral payload
- [ ] Implement remote affiliate attribution
- [ ] Capture attribution at invoice creation
- [ ] Snapshot revenue policy at invoice creation
- [ ] Calculate revenue in integer minor units
- [ ] Add basis-point validation
- [ ] Add idempotent revenue processing
- [ ] Add refund reversal entries
- [ ] Add chargeback reversal entries
- [ ] Add provider settlement reporting
- [ ] Add affiliate settlement reporting
- [ ] Add optional X402 direct split

## Phase 8: Testing

- [ ] Unit test WebFinger parsing
- [ ] Unit test NodeInfo response
- [ ] Unit test actor serialization
- [ ] Unit test ActivityPub video serialization
- [ ] Unit test signature generation
- [ ] Unit test signature verification
- [ ] Unit test SSRF protections
- [ ] Unit test referral verification
- [ ] Unit test revenue calculations
- [ ] Integration test two PPV Stream instances
- [ ] Verify no remote media file is downloaded
- [ ] Verify no remote HLS segment is stored
- [ ] Verify no local playback session is created for remote video
- [ ] Verify no local payment is accepted for remote video
- [ ] Verify blocked instances are excluded
- [ ] Verify duplicate activities are idempotent

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

1. Reject local playback requests for remote videos (stream.rs)
2. Implement `Reject` (manual Follow moderation — Phase 4 remainder)
3. Prevent remote records from entering upload / FFmpeg / storage migration workers
4. Phase 6: domain allow/silence/block moderation admin endpoints

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
