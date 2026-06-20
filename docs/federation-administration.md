# Instance Administration Guide

All admin endpoints require the `X-Federation-Admin-Token` header.

```sh
TOKEN="your-admin-token"
BASE="https://ppv.example.com"
curl -s -H "X-Federation-Admin-Token: $TOKEN" "$BASE/<endpoint>" | jq .
```

---

## Overview dashboard

```sh
GET /api/federation/admin/overview
```

Returns live counts: remote actors, active follows, remote videos,
pending/failed deliveries, and configured domain rules.

---

## Known instances

```sh
GET /api/federation/admin/instances[?limit=50&offset=0]
```

Lists every remote domain with actor and video counts, and any active
domain rule.

---

## Activity log

```sh
GET /api/federation/admin/activities[?limit=50&offset=0]
```

Returns recent federation activities (both inbound and outbound) with
type, actor URI, direction, and processing status.

---

## Delivery queue

```sh
# List all jobs
GET /api/federation/admin/delivery[?limit=50&offset=0]

# Reset a failed job
POST /api/federation/admin/delivery/<job-uuid>/retry
```

Failed jobs can be retried once.  The attempt counter resets to zero so
the full retry budget (10 attempts) is available again.

---

## Remote video cache

```sh
# Soft-delete all cached videos from a domain
DELETE /api/federation/admin/remote-videos/<domain>
```

Sets `is_deleted = TRUE` and `availability_status = 'deleted'` for all
entries from `<domain>`.  Videos will be re-indexed if the instance later
re-publishes them and no block rule is active.

---

## Follow management

```sh
# Reject a pending or accepted follow
POST /api/federation/follows/<follow-uuid>/reject
```

Sends a `Reject{Follow}` ActivityPub activity to the follower's inbox and
sets the follow status to `rejected`.

---

## Revenue share policies

```sh
# List all policies
GET /api/federation/admin/revenue/policies

# Add or update a policy
POST /api/federation/admin/revenue/policies
Content-Type: application/json
{ "domain": "partner.example", "share_basis_points": 500 }

# Settlement reports
GET /api/federation/admin/revenue/provider-report
GET /api/federation/admin/revenue/affiliate-report
```

`share_basis_points` must be in \[0, 5000\] (0–50 %).

---

## Domain moderation

See [federation-moderation.md](federation-moderation.md) for the full
domain rule workflow.
