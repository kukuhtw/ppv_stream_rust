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
- [~] Add federation configuration to `Config`
- [ ] Add configuration validation
- [ ] Add federation database migration
- [ ] Add federation module skeleton
- [ ] Add federation feature flag to application routing
- [ ] Add unit tests for configuration defaults

## Phase 1: Discovery

- [ ] Implement `GET /.well-known/webfinger`
- [ ] Implement `GET /.well-known/nodeinfo`
- [ ] Implement `GET /nodeinfo/2.1`
- [ ] Implement local ActivityPub actor endpoint
- [ ] Return ActivityPub content type
- [ ] Add discovery unit tests
- [ ] Add disabled-federation behavior tests

## Phase 2: Remote Index Storage

- [ ] Create `federation_instances`
- [ ] Create `federation_actors`
- [ ] Create `federation_activities`
- [ ] Create `federation_delivery_jobs`
- [ ] Create `remote_video_catalog`
- [ ] Create `federation_domain_rules`
- [ ] Add indexes and constraints
- [ ] Confirm remote catalog has no media storage columns
- [ ] Add migration rollback documentation

## Phase 3: ActivityPub Identity and Security

- [ ] Generate local actor keys
- [ ] Encrypt actor private keys at rest
- [ ] Implement HTTP Signature creation
- [ ] Implement HTTP Signature verification
- [ ] Implement Digest verification
- [ ] Add replay protection
- [ ] Add maximum request age validation
- [ ] Add federation payload limits
- [ ] Add SSRF-safe remote resolver
- [ ] Block private and local address ranges

## Phase 4: Follow Federation

- [ ] Implement actor inbox
- [ ] Implement shared inbox
- [ ] Implement actor outbox
- [ ] Implement followers collection
- [ ] Implement following collection
- [ ] Implement `Follow`
- [ ] Implement `Accept`
- [ ] Implement `Reject`
- [ ] Implement `Undo`
- [ ] Add activity deduplication
- [ ] Add delivery queue worker
- [ ] Add exponential retry with jitter

## Phase 5: Video Index Federation

- [ ] Define public ActivityPub video index object
- [ ] Publish `Create` when a local video becomes public
- [ ] Publish `Update` when public metadata changes
- [ ] Publish `Delete` when a video is removed or unavailable
- [ ] Process remote video `Create`
- [ ] Process remote video `Update`
- [ ] Process remote video `Delete`
- [ ] Build combined local and remote catalog query
- [ ] Add `hosting_type` to catalog responses
- [ ] Display origin instance for remote videos
- [ ] Add canonical origin watch and checkout links
- [ ] Reject local payment requests for remote videos
- [ ] Reject local playback requests for remote videos
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

## Immediate Implementation Order

The first implementation batch follows this order:

1. Federation configuration
2. Configuration validation
3. Database migration
4. Federation module skeleton
5. WebFinger endpoint
6. NodeInfo endpoints
7. Local actor endpoint
8. Unit tests

## Definition of Done for the First Batch

The first batch is complete when:

- Federation is disabled by default
- Federation can be enabled using environment variables
- The local domain and base URL are validated
- Database tables for the index-only federation foundation exist
- WebFinger can resolve a local user
- NodeInfo reports the PPV Stream software and federation status
- A local actor endpoint returns valid ActivityPub JSON
- No remote media download or playback logic is introduced
- Unit tests cover basic discovery behavior
