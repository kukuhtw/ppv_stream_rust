# Federation Monitoring Metrics

## Key queries

All queries run against the PostgreSQL database.  Consider scheduling
them as alerting rules in your observability stack.

### Delivery queue health

```sql
-- Jobs waiting to be delivered
SELECT COUNT(*) AS queued
FROM federation_delivery_jobs
WHERE status = 'queued';

-- Failed jobs (exhausted all retries)
SELECT COUNT(*) AS failed
FROM federation_delivery_jobs
WHERE status = 'failed';

-- Jobs overdue by more than 5 minutes
SELECT COUNT(*) AS overdue
FROM federation_delivery_jobs
WHERE status = 'queued'
  AND next_attempt_at < NOW() - INTERVAL '5 minutes';
```

**Alert thresholds (recommended)**:
* `failed > 10` — review blocked or unreachable remote instances.
* `overdue > 5` — delivery worker may have stopped; check application logs.

---

### Activity processing lag

```sql
-- Inbound activities still pending after 10 minutes
SELECT COUNT(*) AS stalled
FROM federation_activities
WHERE direction = 'inbound'
  AND processing_status = 'pending'
  AND created_at < NOW() - INTERVAL '10 minutes';
```

---

### Remote instance connectivity

```sql
-- Domains with recent delivery failures
SELECT
    SUBSTR(target_inbox_url, 1, 60) AS inbox,
    COUNT(*) AS failures,
    MAX(last_error) AS last_error
FROM federation_delivery_jobs
WHERE status = 'failed'
  AND updated_at > NOW() - INTERVAL '24 hours'
GROUP BY SUBSTR(target_inbox_url, 1, 60)
ORDER BY failures DESC
LIMIT 20;
```

---

### Content index growth

```sql
-- Remote videos indexed in the last 24 hours
SELECT COUNT(*) AS new_remote_videos
FROM remote_video_catalog
WHERE fetched_at > NOW() - INTERVAL '24 hours'
  AND is_deleted = FALSE;

-- Total federated catalog size
SELECT
    'local'  AS source, COUNT(*) AS count
    FROM videos WHERE federation_visibility = 'public'
UNION ALL
SELECT
    'remote' AS source, COUNT(*) AS count
    FROM remote_video_catalog WHERE is_deleted = FALSE;
```

---

### Revenue share pipeline

```sql
-- Pending revenue shares (money owed to providers)
SELECT
    referring_domain,
    COUNT(*) AS invoices,
    SUM(share_cents) AS total_cents
FROM federation_revenue_shares
WHERE status = 'pending'
GROUP BY referring_domain;

-- Ledger integrity check (credits should equal debits + reversals per share)
SELECT rs.id, rs.share_cents,
       COALESCE(SUM(le.amount_cents) FILTER (WHERE le.entry_type = 'credit'), 0) AS credits,
       COALESCE(SUM(le.amount_cents) FILTER (WHERE le.entry_type IN ('debit','refund','chargeback')), 0) AS debits
FROM federation_revenue_shares rs
LEFT JOIN revenue_ledger_entries le ON le.revenue_share_id = rs.id
GROUP BY rs.id, rs.share_cents
HAVING SUM(le.amount_cents) FILTER (WHERE le.entry_type = 'credit') IS NULL
    OR SUM(le.amount_cents) FILTER (WHERE le.entry_type = 'credit') != rs.share_cents;
```

An empty result from the integrity check means the ledger is balanced.

---

## Log events to watch

| Log message | Level | Meaning |
|---|---|---|
| `index-only federation enabled` | INFO | Server started with federation active. |
| `federation delivery worker started` | INFO | Background worker running. |
| `Follow accepted` | INFO | New follower relationship established. |
| `Reject{Follow} queued` | INFO | Admin rejected a follow request. |
| `inbox: rejected activity from blocked/suspended domain` | INFO | Domain rule enforced. |
| `delivery skipped: domain … is blocked/suspended` | INFO | Outbound delivery prevented by domain rule. |
| `federation publish_create failed` | WARN | Could not broadcast a new public video. |
| `activity processing failed` | WARN | Inbound activity handler error. |
| `federation delivery worker` | ERROR | Worker loop error — may stop processing. |

---

## Admin API polling

```sh
#!/usr/bin/env sh
# Quick health snapshot
TOKEN="${FEDERATION_ADMIN_TOKEN}"
BASE="${FEDERATION_BASE_URL}"

curl -s -H "X-Federation-Admin-Token: $TOKEN" "$BASE/api/federation/admin/overview"
echo
curl -s -H "X-Federation-Admin-Token: $TOKEN" \
     "$BASE/api/federation/admin/delivery?limit=5" | jq '.jobs[] | select(.status=="failed")'
```
