# Federation Moderation Guide

## Domain rules

Domain rules let you control which remote instances can federate with
yours.  Each rule applies to a single bare hostname (no path, no `@`).

### Rule types

| Action | Inbound activities | Outbound delivery | Remote videos |
|---|---|---|---|
| `allow` | accepted (default allowlist entry) | delivered | indexed |
| `silence` | accepted | delivered | indexed but not in default catalog |
| `reject_media` | accepted | delivered | not indexed |
| `suspend` | rejected (403) | failed immediately | existing entries hidden |
| `block` | rejected (403) | failed immediately | existing entries hidden |

> **Note**: `silence` and `reject_media` are enforced at the application
> layer; `suspend` and `block` stop all federation activity at the inbox
> and delivery worker level.

### Adding a rule

```sh
curl -X POST https://ppv.example.com/api/federation/admin/domain-rules \
  -H "X-Federation-Admin-Token: $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"domain":"spam.example","action":"block","reason":"Repeated unsolicited activities"}'
```

### Listing rules

```sh
curl https://ppv.example.com/api/federation/admin/domain-rules \
  -H "X-Federation-Admin-Token: $TOKEN" | jq .
```

### Removing a rule

```sh
curl -X DELETE \
  https://ppv.example.com/api/federation/admin/domain-rules/spam.example \
  -H "X-Federation-Admin-Token: $TOKEN"
```

---

## Handling unwanted followers

To remove a specific follower's access, use the reject follow endpoint:

```sh
# 1. Find the follow record UUID
SELECT ff.id, fa.actor_uri, ff.status
FROM federation_follows ff
JOIN federation_actors fa ON fa.id = ff.follower_actor_id
WHERE fa.domain = 'unwanted.example';

# 2. Reject the follow
curl -X POST https://ppv.example.com/api/federation/follows/<uuid>/reject \
  -H "X-Federation-Admin-Token: $TOKEN"
```

---

## Purging cached remote content

If you block a domain after its videos have been indexed:

```sh
curl -X DELETE \
  https://ppv.example.com/api/federation/admin/remote-videos/spam.example \
  -H "X-Federation-Admin-Token: $TOKEN"
```

This soft-deletes all cached video entries from that domain.  The
`DELETE` activity is not sent to the origin; entries are simply marked
as deleted locally.

---

## Escalation workflow

Recommended escalation path for problematic instances:

1. **Monitor**: check the activity log for unusual volume from a domain.
2. **Silence**: hide their videos from the default catalog while
   continuing to receive activities for review.
3. **Suspend**: stop all federation activity and hide existing content.
4. **Block**: strongest option — same as suspend plus no inbound
   activities are acknowledged.
5. **Purge**: delete cached remote video entries for the domain.
