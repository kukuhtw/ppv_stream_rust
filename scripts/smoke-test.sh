#!/usr/bin/env bash
# Smoke test: start the real binary against an empty PostgreSQL database and
# exercise the main HTTP flows (public pages, user, wallet deposit approval,
# admin, FX override). Fails on any HTTP 5xx, unexpected status, panic, or if
# the server does not come up again after a restart.
#
# Usage: scripts/smoke-test.sh <path-to-binary>
# Env:   SMOKE_DATABASE_URL  empty database the app will migrate itself
#
# All credentials are generated per run and only live inside the CI job.
# Test accounts use example.com addresses; no real personal data is used.
set -uo pipefail

BIN="${1:?usage: smoke-test.sh <binary>}"
: "${SMOKE_DATABASE_URL:?SMOKE_DATABASE_URL must point to an empty database}"

PORT="${SMOKE_PORT:-18080}"
BASE="http://127.0.0.1:${PORT}"
WORK="$(mktemp -d)"
LOG="${SMOKE_LOG:-$WORK/server.log}"
USER_JAR="$WORK/user.cookies"
ADMIN_JAR="$WORK/admin.cookies"
FAILURES=0
SERVER_PID=""

rand() { openssl rand -hex "$1"; }

export DATABASE_URL="$SMOKE_DATABASE_URL"
export BIND="127.0.0.1:${PORT}"
export BASE_URL="$BASE"
export HMAC_SECRET="$(rand 32)"
export SESSION_COOKIE_SECURE=false
export ADMIN_BOOTSTRAP_TOKEN="$(rand 16)"
export ADMIN_BOOTSTRAP_EMAIL="smoke-admin@example.com"
export ADMIN_BOOTSTRAP_PASSWORD="Adm1n-$(rand 8)"
export STORAGE_DIR="$WORK/storage" MEDIA_DIR="$WORK/media" HLS_ROOT="$WORK/hls" TMP_DIR="$WORK/tmp"
export PUBLIC_DIR="${PUBLIC_DIR:-$PWD/public}"
export RUST_LOG="${RUST_LOG:-info}"
mkdir -p "$STORAGE_DIR" "$MEDIA_DIR" "$HLS_ROOT" "$TMP_DIR"

USER_NAME="smoke_user_$(rand 3)"
USER_EMAIL="${USER_NAME}@example.com"
USER_PASS="Us3r-$(rand 8)"

pass() { printf '  \033[32mPASS\033[0m %s\n' "$1"; }
fail() { printf '  \033[31mFAIL\033[0m %s\n' "$1"; FAILURES=$((FAILURES + 1)); }

start_server() {
  "$BIN" >>"$LOG" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 90); do
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      echo "server exited during startup"; tail -n 60 "$LOG"; return 1
    fi
    if [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/health")" = "200" ]; then
      return 0
    fi
    sleep 1
  done
  echo "server did not answer /health within 90s"; tail -n 60 "$LOG"; return 1
}

stop_server() {
  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID"; wait "$SERVER_PID" 2>/dev/null || true
  fi
  SERVER_PID=""
}
trap stop_server EXIT

# req <name> <expected-status-regex> <method> <path> [curl args...]
# Body is left in $BODY, status in $CODE, Location header in $LOC.
req() {
  local name="$1" expect="$2" method="$3" path="$4"; shift 4
  local out="$WORK/body" hdr="$WORK/headers"
  CODE="$(curl -s -o "$out" -D "$hdr" -w '%{http_code}' -X "$method" "$@" "$BASE$path")"
  BODY="$(cat "$out")"
  LOC="$(grep -i '^location:' "$hdr" | tr -d '\r' | sed 's/^[Ll]ocation: *//')"
  if [[ "$CODE" =~ ^5 ]]; then
    fail "$name: HTTP $CODE (server error) ${BODY:0:200}"; return 1
  elif [[ ! "$CODE" =~ ^($expect)$ ]]; then
    fail "$name: expected $expect, got $CODE ${BODY:0:200}"; return 1
  fi
  pass "$name ($CODE)"; return 0
}

# json_ok <name>: assert the last body is JSON with ok == true
json_ok() {
  if [ "$(jq -r '.ok // empty' <<<"$BODY" 2>/dev/null)" = "true" ]; then pass "$1"
  else fail "$1: ${BODY:0:200}"; fi
}

# expect_loc <name> <substring>: assert redirect target
expect_loc() {
  if [[ "$LOC" == *"$2"* ]]; then pass "$1 -> $LOC"; else fail "$1: redirect '$LOC' lacks '$2'"; fi
}

ORIGIN=(-H "Origin: $BASE")

echo "== Start (empty database, app runs its own migrations)"
if ! start_server; then exit 1; fi
pass "server started, /health ok"

echo "== Public"
req "GET /health" 200 GET /health
if [ "$BODY" = "ok" ]; then pass "health body is ok"; else fail "health body: ${BODY:0:100}"; fi
req "GET / redirects to frontend" '30[1-8]' GET /
for page in index.html browse.html watch.html wallet.html dashboard.html affiliate.html \
            auth/register.html admin/login.html admin/dashboard.html admin/settings.html; do
  req "GET /public/$page" 200 GET "/public/$page"
done
req "GET /api/videos" 200 GET /api/videos
req "GET /api/kurs" 200 GET /api/kurs
if jq -e '.usd_to_idr > 0' <<<"$BODY" >/dev/null 2>&1; then pass "kurs has usd_to_idr"; else fail "kurs body: ${BODY:0:200}"; fi
req "GET /api/pay/providers" 200 GET /api/pay/providers
req "GET /api/me (anonymous)" '200|401|403' GET /api/me
req "GET /api/pay/all_options unknown video" '200|400|401|403|404' GET "/api/pay/all_options?video_id=does-not-exist"

echo "== User"
req "POST /auth/register" '30[1-8]' POST /auth/register "${ORIGIN[@]}" \
  --data-urlencode "username=$USER_NAME" --data-urlencode "email=$USER_EMAIL" --data-urlencode "password=$USER_PASS" \
  && expect_loc "register" "status=ok"
req "POST /auth/login" '30[1-8]' POST /auth/login "${ORIGIN[@]}" -c "$USER_JAR" \
  --data-urlencode "email=$USER_EMAIL" --data-urlencode "password=$USER_PASS" \
  && expect_loc "login" "dashboard.html"
U=(-b "$USER_JAR" "${ORIGIN[@]}")
req "GET /api/me" 200 GET /api/me "${U[@]}"
if [[ "$BODY" == *"$USER_NAME"* ]]; then pass "session belongs to new user"; else fail "/api/me: ${BODY:0:200}"; fi
for p in /api/wallet/balance /api/wallet/transactions /api/wallet/deposit_info /api/profile \
         /api/my_videos /api/affiliate/summary /api/chat/conversations; do
  req "GET $p" 200 GET "$p" "${U[@]}"
done
req "POST /api/wallet/deposit" 200 POST /api/wallet/deposit "${U[@]}" -H 'Content-Type: application/json' \
  -d '{"amount_cents":1000,"note":"smoke test","transfer_ref":"SMOKE-REF-1","sender_name":"Smoke Tester"}' \
  && json_ok "deposit accepted"
TXN_ID="$(jq -r '.txn_id // empty' <<<"$BODY" 2>/dev/null)"
req "POST without Origin is blocked (CSRF)" 403 POST /api/wallet/deposit -b "$USER_JAR" \
  -H 'Content-Type: application/json' -d '{"amount_cents":1000}'
req "GET /admin/fx as normal user is refused" '200|401|403|30[1-8]' GET /admin/fx "${U[@]}"
if [ "$(jq -r '.ok // empty' <<<"$BODY" 2>/dev/null)" = "true" ]; then fail "normal user can read /admin/fx"; else pass "normal user cannot read /admin/fx"; fi

echo "== Admin"
req "GET /setup_admin wrong token" 200 GET "/setup_admin?token=wrong"
if [[ "$BODY" == *Unauthorized* ]]; then pass "wrong bootstrap token refused"; else fail "setup_admin wrong token: ${BODY:0:200}"; fi
req "GET /setup_admin" 200 GET "/setup_admin?token=$ADMIN_BOOTSTRAP_TOKEN"
if [[ "$BODY" == *OK* ]]; then pass "admin bootstrapped"; else fail "setup_admin: ${BODY:0:200}"; fi
req "GET /setup_admin again is closed" 200 GET "/setup_admin?token=$ADMIN_BOOTSTRAP_TOKEN"
if [[ "$BODY" == *Forbidden* ]]; then pass "bootstrap closed after first admin"; else fail "setup_admin reuse: ${BODY:0:200}"; fi
req "POST /admin/login" '30[1-8]' POST /admin/login "${ORIGIN[@]}" -c "$ADMIN_JAR" \
  --data-urlencode "email=$ADMIN_BOOTSTRAP_EMAIL" --data-urlencode "password=$ADMIN_BOOTSTRAP_PASSWORD" \
  && expect_loc "admin login" "admin/dashboard.html"
A=(-b "$ADMIN_JAR" "${ORIGIN[@]}")
for p in /admin/payments /admin/payment_settings /admin/smtp /admin/fx \
         "/admin/wallet/transactions?txn_type=deposit&status=pending"; do
  req "GET $p" 200 GET "$p" "${A[@]}"
done
if [ -n "$TXN_ID" ]; then
  req "POST approve deposit #$TXN_ID" 200 POST "/admin/wallet/transactions/$TXN_ID/approve" "${A[@]}" \
    -H 'Content-Type: application/json' -d '{"admin_note":"smoke test"}' && json_ok "deposit approved"
  req "GET /api/wallet/balance after approval" 200 GET /api/wallet/balance "${U[@]}"
  if jq -e '.balance_cents == 1000' <<<"$BODY" >/dev/null 2>&1; then pass "balance credited (\$10.00)"; else fail "balance after approval: ${BODY:0:200}"; fi
else
  fail "no txn_id from deposit; approval flow skipped"
fi
req "POST /admin/fx/override USD/IDR" 200 POST /admin/fx/override "${A[@]}" \
  -H 'Content-Type: application/json' -d '{"pair":"USD/IDR","rate":16123}' && json_ok "fx override saved"
req "GET /api/kurs after override" 200 GET /api/kurs
if jq -e '.usd_to_idr == 16123' <<<"$BODY" >/dev/null 2>&1; then pass "override visible in /api/kurs"; else fail "kurs after override: ${BODY:0:200}"; fi

echo "== Restart (migrations must be idempotent)"
stop_server
if start_server; then pass "server restarted on migrated database"; else fail "restart failed"; fi
req "GET /api/videos after restart" 200 GET /api/videos
req "GET /api/kurs keeps override after restart" 200 GET /api/kurs
if jq -e '.usd_to_idr == 16123' <<<"$BODY" >/dev/null 2>&1; then pass "override persisted"; else fail "kurs after restart: ${BODY:0:200}"; fi
stop_server

echo "== Server log"
if grep -qE 'panicked at|thread .* panicked' "$LOG"; then
  fail "panic in server log"; grep -nE -A5 'panicked' "$LOG" | head -40
else
  pass "no panics"
fi
ERRORS="$(grep -cE '\bERROR\b' "$LOG" || true)"
echo "  ERROR lines in log: $ERRORS (shown below; outbound FX fetches may fail without failing the test)"
grep -nE '\bERROR\b' "$LOG" | head -20 || true

echo
if [ "$FAILURES" -gt 0 ]; then
  echo "Smoke test FAILED: $FAILURES check(s)"; exit 1
fi
echo "Smoke test passed"
