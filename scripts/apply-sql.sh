#!/usr/bin/env bash
# Apply sql/ then migrations/ to $DATABASE_URL with psql (CI only).
# The sqlx compile-time macros need this schema to type-check queries.
set -euo pipefail
: "${DATABASE_URL:?DATABASE_URL must be set}"

for dir in sql migrations; do
  [ -d "$dir" ] || continue
  shopt -s nullglob
  for file in "$dir"/*.sql; do
    echo "Applying $file"
    psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q -f "$file"
  done
  shopt -u nullglob
done
