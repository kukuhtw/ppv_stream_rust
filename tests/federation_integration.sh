#!/usr/bin/env bash
# Federation integration test: two PPV Stream instances (A and B) federating.
#
# Prerequisites:
#   docker compose -f docker-compose.federation-test.yml up --build -d
#
# Then run:
#   bash tests/federation_integration.sh
#
# Exit codes: 0 = all tests passed, 1 = one or more tests failed.

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
A_URL="${INSTANCE_A_URL:-http://localhost:18082}"
B_URL="${INSTANCE_B_URL:-http://localhost:18083}"
A_ADMIN_TOKEN="${INSTANCE_A_ADMIN_TOKEN:-fed-test-token-a}"
B_ADMIN_TOKEN="${INSTANCE_B_ADMIN_TOKEN:-fed-test-token-b}"
A_BOOTSTRAP_TOKEN="${INSTANCE_A_BOOTSTRAP_TOKEN:-bootstrap-a}"
B_BOOTSTRAP_TOKEN="${INSTANCE_B_BOOTSTRAP_TOKEN:-bootstrap-b}"

PASS=0
FAIL=0

# ── Helpers ───────────────────────────────────────────────────────────────────
green() { printf '\033[32m%s\033[0m\n' "$*"; }
red()   { printf '\033[31m%s\033[0m\n' "$*"; }

ok() {
  green "  PASS: $1"
  PASS=$((PASS + 1))
}

fail() {
  red "  FAIL: $1"
  red "        $2"
  FAIL=$((FAIL + 1))
}

assert_status() {
  local label="$1"
  local expected="$2"
  local actual="$3"
  local body="$4"
  if [ "$actual" = "$expected" ]; then
    ok "$label (HTTP $actual)"
  else
    fail "$label" "expected HTTP $expected, got HTTP $actual. Body: $body"
  fi
}

assert_contains() {
  local label="$1"
  local needle="$2"
  local haystack="$3"
  if echo "$haystack" | grep -q "$needle"; then
    ok "$label"
  else
    fail "$label" "expected '$needle' in: $haystack"
  fi
}

wait_for() {
  local url="$1"
  local label="$2"
  local max=60
  local count=0
  printf "  Waiting for %s" "$label"
  until curl -fsS "$url" >/dev/null 2>&1; do
    count=$((count + 1))
    if [ $count -ge $max ]; then
      echo ""
      fail "health check" "$label did not become healthy within ${max}s"
      exit 1
    fi
    printf "."
    sleep 1
  done
  echo ""
  ok "$label is healthy"
}

# Wait for both instances to be reachable before running any tests.
wait_for "$A_URL/health" "Instance A"
wait_for "$B_URL/health" "Instance B"

echo ""
echo "════════════════════════════════════════"
echo " Phase 1: Discovery endpoints"
echo "════════════════════════════════════════"

# ── NodeInfo ──────────────────────────────────────────────────────────────────
for INST in A B; do
  URL=$([ "$INST" = "A" ] && echo "$A_URL" || echo "$B_URL")
  RESP=$(curl -sS "$URL/.well-known/nodeinfo")
  assert_contains "Instance $INST NodeInfo well-known" "nodeinfo" "$RESP"
  NI=$(curl -sS "$URL/nodeinfo/2.1")
  assert_contains "Instance $INST NodeInfo 2.1 software name" "ppv_stream_rust" "$NI"
  assert_contains "Instance $INST NodeInfo federation mode" "index-only" "$NI"
done

echo ""
echo "════════════════════════════════════════"
echo " Phase 2: Bootstrap admin + create users"
echo "════════════════════════════════════════"

# Bootstrap admin on instance A
STATUS=$(curl -sS -o /dev/null -w "%{http_code}" -X POST "$A_URL/api/admin/bootstrap" \
  -H "Content-Type: application/json" \
  -d "{\"token\":\"$A_BOOTSTRAP_TOKEN\",\"email\":\"admin@instance-a.test\",\"password\":\"TestPass123!\",\"username\":\"admin-a\"}" || echo "000")
# 200 = created, 409 = already exists — both are acceptable
if [ "$STATUS" = "200" ] || [ "$STATUS" = "409" ]; then
  ok "Instance A admin bootstrap (HTTP $STATUS)"
else
  fail "Instance A admin bootstrap" "HTTP $STATUS"
fi

# Bootstrap admin on instance B
STATUS=$(curl -sS -o /dev/null -w "%{http_code}" -X POST "$B_URL/api/admin/bootstrap" \
  -H "Content-Type: application/json" \
  -d "{\"token\":\"$B_BOOTSTRAP_TOKEN\",\"email\":\"admin@instance-b.test\",\"password\":\"TestPass123!\",\"username\":\"admin-b\"}" || echo "000")
if [ "$STATUS" = "200" ] || [ "$STATUS" = "409" ]; then
  ok "Instance B admin bootstrap (HTTP $STATUS)"
else
  fail "Instance B admin bootstrap" "HTTP $STATUS"
fi

# Login to instance A as admin to get session token
A_TOKEN=$(curl -sS -X POST "$A_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@instance-a.test","password":"TestPass123!"}' | grep -o '"token":"[^"]*"' | cut -d'"' -f4 || echo "")

# Login to instance B as admin
B_TOKEN=$(curl -sS -X POST "$B_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@instance-b.test","password":"TestPass123!"}' | grep -o '"token":"[^"]*"' | cut -d'"' -f4 || echo "")

echo ""
echo "════════════════════════════════════════"
echo " Phase 3: Initialize federation actors"
echo "════════════════════════════════════════"

# Make admin-a federation-discoverable by updating their profile
# (enable federation_enabled + discoverable on the user record via admin API)
if [ -n "$A_TOKEN" ]; then
  # Use admin API to enable federation for admin-a user
  STATUS=$(curl -sS -o /dev/null -w "%{http_code}" -X PUT "$A_URL/api/admin/users/admin-a/federation" \
    -H "Authorization: Bearer $A_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"federation_enabled":true,"discoverable":true}' || echo "000")
  # May not exist; if so we try the profile update endpoint
  if [ "$STATUS" != "200" ]; then
    curl -sS -X PUT "$A_URL/api/users/admin-a/profile" \
      -H "Authorization: Bearer $A_TOKEN" \
      -H "Content-Type: application/json" \
      -d '{"federation_enabled":true,"discoverable":true}' >/dev/null 2>&1 || true
  fi
fi

if [ -n "$B_TOKEN" ]; then
  STATUS=$(curl -sS -o /dev/null -w "%{http_code}" -X PUT "$B_URL/api/admin/users/admin-b/federation" \
    -H "Authorization: Bearer $B_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"federation_enabled":true,"discoverable":true}' || echo "000")
  if [ "$STATUS" != "200" ]; then
    curl -sS -X PUT "$B_URL/api/users/admin-b/profile" \
      -H "Authorization: Bearer $B_TOKEN" \
      -H "Content-Type: application/json" \
      -d '{"federation_enabled":true,"discoverable":true}' >/dev/null 2>&1 || true
  fi
fi

# Generate RSA keys for admin-a actor on instance A
RESP=$(curl -sS -X POST "$A_URL/api/federation/admin/actors/init" \
  -H "X-Federation-Admin-Token: $A_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin-a"}')
assert_contains "Instance A actor init" "actor_url" "$RESP"

# Generate RSA keys for admin-b actor on instance B
RESP=$(curl -sS -X POST "$B_URL/api/federation/admin/actors/init" \
  -H "X-Federation-Admin-Token: $B_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin-b"}')
assert_contains "Instance B actor init" "actor_url" "$RESP"

echo ""
echo "════════════════════════════════════════"
echo " Phase 4: WebFinger discovery"
echo "════════════════════════════════════════"

# Now that federation is enabled and actors are initialised, WebFinger should work
WF=$(curl -sS "$A_URL/.well-known/webfinger?resource=acct:admin-a@instance-a")
assert_contains "Instance A WebFinger subject" "instance-a" "$WF"

WF=$(curl -sS "$B_URL/.well-known/webfinger?resource=acct:admin-b@instance-b")
assert_contains "Instance B WebFinger subject" "instance-b" "$WF"

echo ""
echo "════════════════════════════════════════"
echo " Phase 5: Actor document resolution"
echo "════════════════════════════════════════"

ACTOR_B=$(curl -sS -H "Accept: application/activity+json" "$B_URL/users/admin-b")
assert_contains "Instance B actor type" '"type":"Person"' "$ACTOR_B"
assert_contains "Instance B actor inbox" '"inbox"' "$ACTOR_B"
assert_contains "Instance B actor publicKey" '"publicKey"' "$ACTOR_B"

ACTOR_A=$(curl -sS -H "Accept: application/activity+json" "$A_URL/users/admin-a")
assert_contains "Instance A actor type" '"type":"Person"' "$ACTOR_A"
assert_contains "Instance A actor publicKey" '"publicKey"' "$ACTOR_A"

echo ""
echo "════════════════════════════════════════"
echo " Phase 6: Follow federation"
echo "════════════════════════════════════════"

# Instance A follows Instance B's admin-b actor
FOLLOW_RESP=$(curl -sS -X POST "$A_URL/api/federation/admin/follow" \
  -H "X-Federation-Admin-Token: $A_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"local_username\":\"admin-a\",\"remote_actor_url\":\"http://instance-b:8080/users/admin-b\"}")
assert_contains "Instance A queued follow to B" "follow_activity_uri" "$FOLLOW_RESP"

# Wait for delivery worker to send the Follow (poll delivery queue on A)
echo "  Waiting for Follow delivery..."
DELIVERED=false
for i in $(seq 1 40); do
  QUEUE=$(curl -sS "$A_URL/api/federation/admin/delivery?limit=10" \
    -H "X-Federation-Admin-Token: $A_ADMIN_TOKEN")
  if echo "$QUEUE" | grep -q '"status":"delivered"'; then
    DELIVERED=true
    break
  fi
  sleep 3
done
if $DELIVERED; then
  ok "Follow activity delivered from A to B"
else
  fail "Follow delivery" "delivery did not complete within 120s"
fi

# Verify B accepted the follow (B should show A's inbox in followers collection)
sleep 2
FOLLOWERS=$(curl -sS "$B_URL/users/admin-b/followers" \
  -H "Accept: application/activity+json")
assert_contains "Instance B followers includes admin-a actor" "instance-a" "$FOLLOWERS"

# Verify A received an Accept from B (delivery job for Accept should appear on B)
QUEUE_B=$(curl -sS "$B_URL/api/federation/admin/delivery?limit=10" \
  -H "X-Federation-Admin-Token: $B_ADMIN_TOKEN")
assert_contains "Instance B queued Accept back to A" "Accept" "$QUEUE_B"

echo ""
echo "════════════════════════════════════════"
echo " Phase 7: Video index federation"
echo "════════════════════════════════════════"

# Create a video on instance B (as admin-b user)
VIDEO_RESP=""
if [ -n "$B_TOKEN" ]; then
  VIDEO_RESP=$(curl -sS -X POST "$B_URL/api/videos" \
    -H "Authorization: Bearer $B_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"title":"Federated Test Video","description":"Test video for federation","price_cents":1000}' || echo "")
fi
VIDEO_ID=$(echo "$VIDEO_RESP" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4 || echo "")

if [ -n "$VIDEO_ID" ]; then
  ok "Video created on Instance B (id: $VIDEO_ID)"

  # Set federation_visibility to 'public' — this triggers publish_create
  UPDATE_RESP=$(curl -sS -X PUT "$B_URL/api/videos/$VIDEO_ID" \
    -H "Authorization: Bearer $B_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"federation_visibility":"public"}' || echo "")
  assert_contains "Video federation_visibility set to public on B" "ok\|success\|$VIDEO_ID" "$UPDATE_RESP"

  # Wait for B to deliver the Create activity to A
  echo "  Waiting for Create activity delivery from B to A..."
  DELIVERED=false
  for i in $(seq 1 40); do
    QUEUE_B=$(curl -sS "$B_URL/api/federation/admin/delivery?limit=20" \
      -H "X-Federation-Admin-Token: $B_ADMIN_TOKEN")
    if echo "$QUEUE_B" | grep -q '"status":"delivered"'; then
      DELIVERED=true
      break
    fi
    sleep 3
  done
  if $DELIVERED; then
    ok "Create activity delivered from B to A"
  else
    fail "Create delivery" "Create activity delivery did not complete within 120s"
  fi

  # Verify the remote video appears in Instance A's federation catalog
  sleep 2
  CATALOG=$(curl -sS "$A_URL/api/federation/catalog")
  assert_contains "Remote video appears in A's catalog" "remote" "$CATALOG"
  assert_contains "Remote video has hosting_type=remote" '"hosting_type":"remote"' "$CATALOG"
  assert_contains "Remote video has checkout_url" "checkout_url" "$CATALOG"
  assert_contains "Remote video has watch_url" "watch_url" "$CATALOG"

  echo ""
  echo "════════════════════════════════════════"
  echo " Phase 8: Rejection guards"
  echo "════════════════════════════════════════"

  # Extract the remote video's object_uri (used as video_id on A)
  REMOTE_VIDEO_URI=$(curl -sS "$A_URL/api/federation/catalog" | \
    grep -o '"object_uri":"[^"]*"' | head -1 | cut -d'"' -f4 || echo "")

  if [ -n "$REMOTE_VIDEO_URI" ]; then
    # Payment should be rejected for remote videos
    STATUS=$(curl -sS -o /dev/null -w "%{http_code}" \
      "$A_URL/api/pay/options?video_id=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$REMOTE_VIDEO_URI'))" 2>/dev/null || echo "$REMOTE_VIDEO_URI")" || echo "000")
    # Should return 422 with redirect info, or 404, not 200
    if [ "$STATUS" = "422" ] || [ "$STATUS" = "404" ]; then
      ok "Payment rejected for remote video (HTTP $STATUS)"
    else
      fail "Payment rejection" "expected 422 or 404, got HTTP $STATUS"
    fi

    # Playback should be rejected for remote videos
    STATUS=$(curl -sS -o /dev/null -w "%{http_code}" \
      "$A_URL/api/stream/request?video_id=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$REMOTE_VIDEO_URI'))" 2>/dev/null || echo "$REMOTE_VIDEO_URI")" || echo "000")
    if [ "$STATUS" = "422" ] || [ "$STATUS" = "404" ] || [ "$STATUS" = "401" ] || [ "$STATUS" = "403" ]; then
      ok "Playback rejected for remote video (HTTP $STATUS)"
    else
      fail "Playback rejection" "expected 4xx, got HTTP $STATUS"
    fi
  else
    fail "Remote video URI" "could not extract object_uri from catalog"
  fi
else
  fail "Video creation" "no video ID returned from instance B"
fi

echo ""
echo "════════════════════════════════════════"
echo " Phase 9: Domain moderation"
echo "════════════════════════════════════════"

# Block instance-b on instance-a and verify subsequent activities are rejected
BLOCK_RESP=$(curl -sS -X POST "$A_URL/api/federation/admin/domain-rules" \
  -H "X-Federation-Admin-Token: $A_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"domain":"instance-b","action":"block","reason":"integration test block"}')
assert_contains "Domain block created on A for instance-b" "ok" "$BLOCK_RESP"

# Sending an activity from instance-b to A's inbox should now return 403
STATUS=$(curl -sS -o /dev/null -w "%{http_code}" -X POST "$A_URL/inbox" \
  -H "Content-Type: application/activity+json" \
  -d '{"@context":"https://www.w3.org/ns/activitystreams","type":"Create","actor":"http://instance-b:8080/users/admin-b","id":"http://instance-b:8080/activities/test-blocked"}')
assert_status "Blocked domain activity rejected by A inbox" "403" "$STATUS" ""

# Remove the block so we don't break other tests
curl -sS -X DELETE "$A_URL/api/federation/admin/domain-rules/instance-b" \
  -H "X-Federation-Admin-Token: $A_ADMIN_TOKEN" >/dev/null 2>&1 || true

echo ""
echo "════════════════════════════════════════"
echo " Phase 10: Deduplication"
echo "════════════════════════════════════════"

# Posting the same activity twice should not create a duplicate
ACTIVITY_BODY='{"@context":"https://www.w3.org/ns/activitystreams","type":"Create","actor":"http://instance-b:8080/users/admin-b","id":"http://instance-b:8080/activities/dedup-test-001","object":{"type":"Note","id":"http://instance-b:8080/notes/1","content":"dedup test"}}'

# First POST — expect 202 (accepted for processing) or 401 (missing sig, which also means dedup is not the issue)
S1=$(curl -sS -o /dev/null -w "%{http_code}" -X POST "$A_URL/inbox" \
  -H "Content-Type: application/activity+json" \
  -d "$ACTIVITY_BODY")
# Second POST of same activity ID — deduplication should return 202 (already seen) or similar
S2=$(curl -sS -o /dev/null -w "%{http_code}" -X POST "$A_URL/inbox" \
  -H "Content-Type: application/activity+json" \
  -d "$ACTIVITY_BODY")

# Both should be consistent (either both 401 from missing sig, or second is still 202 not 500)
if [ "$S2" != "500" ] && [ "$S2" != "000" ]; then
  ok "Duplicate activity does not cause server error (HTTP $S2)"
else
  fail "Deduplication" "second POST of same activity returned HTTP $S2"
fi

echo ""
echo "════════════════════════════════════════"
echo " Phase 11: Admin API overview"
echo "════════════════════════════════════════"

OV_A=$(curl -sS "$A_URL/api/federation/admin/overview" \
  -H "X-Federation-Admin-Token: $A_ADMIN_TOKEN")
assert_contains "Instance A overview returns known_instances" "known_instances\|instances" "$OV_A"

OV_B=$(curl -sS "$B_URL/api/federation/admin/overview" \
  -H "X-Federation-Admin-Token: $B_ADMIN_TOKEN")
assert_contains "Instance B overview returns known_instances" "known_instances\|instances" "$OV_B"

echo ""
echo "════════════════════════════════════════"
echo " Results"
echo "════════════════════════════════════════"

echo ""
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo ""

if [ $FAIL -eq 0 ]; then
  green "All federation integration tests passed."
  exit 0
else
  red "$FAIL test(s) failed."
  exit 1
fi
