# Federation Troubleshooting Guide

## Activities not being delivered

**Symptoms**: outbound delivery jobs remain in `queued` status; remote
instances are not receiving activities.

**Check**:
```sh
# List failed or stuck delivery jobs
curl https://ppv.example.com/api/federation/admin/delivery \
  -H "X-Federation-Admin-Token: $TOKEN" | jq '.jobs[] | select(.status != "delivered")'
```

**Common causes**:

| Cause | Fix |
|---|---|
| `HMAC_SECRET` not set or changed after key generation | Restore the original `HMAC_SECRET`; private keys encrypted with the old secret will not decrypt. |
| Target domain is blocked | Check domain rules; remove the `block`/`suspend` rule if unintended. |
| Remote inbox URL changed | Remove the stale actor record so it is re-fetched on next Follow. |
| Clock skew > 30 s | Synchronise system clock with NTP. |
| Network firewall blocking outbound HTTPS | Verify outbound port 443 is open from the server. |

Retry a specific failed job:
```sh
curl -X POST https://ppv.example.com/api/federation/admin/delivery/<job-id>/retry \
  -H "X-Federation-Admin-Token: $TOKEN"
```

---

## Inbound activities are rejected (401/403)

**Symptoms**: remote instances report delivery failures; this instance
logs `HTTP Signature verification failed`.

**Check**:
1. Confirm `HMAC_SECRET` is set and the actor's public key PEM is stored
   in `federation_actors`.
2. Verify the server clock is within 30 seconds of the sending server.
3. Check for a `block` or `suspend` domain rule on the sender's domain.

---

## WebFinger returns 404 for a valid user

**Check**:
1. The user must have `federation_enabled = TRUE` and `discoverable = TRUE`.
2. The `FEDERATION_DOMAIN` must match the domain in the WebFinger query.
   ```sql
   SELECT username, federation_enabled, discoverable FROM users WHERE username = 'alice';
   ```

---

## `video not found` on the AP catalog endpoint

`GET /videos/:id` with `Accept: application/activity+json` returns 404 when:
* The video does not exist.
* `federation_visibility` is not `'public'`.

---

## Remote videos not appearing in the catalog

1. Check that the sender's domain is not `blocked`, `suspended`, or
   `reject_media`.
2. Look for inbound `Create` or `Update` activities in the activity log.
3. Verify the activity was processed (status `processed` not `failed`).
4. Query `remote_video_catalog` directly:
   ```sql
   SELECT * FROM remote_video_catalog WHERE origin_domain = 'remote.example';
   ```

---

## Delivery worker not starting

The worker starts only when `FEDERATION_ENABLED=true`.  Check application
logs for `federation delivery worker started`.

If the worker crashes repeatedly, check:
* `HMAC_SECRET` is set.
* The database is reachable and the `federation_delivery_jobs` table exists.

---

## Revenue share not calculating

1. Confirm a policy exists for the referring domain:
   ```sh
   curl https://ppv.example.com/api/federation/admin/revenue/policies \
     -H "X-Federation-Admin-Token: $TOKEN"
   ```
2. Verify `is_active = true` and `share_basis_points > 0`.
3. Check `process_revenue_share` is called from the invoice completion handler.
