#!/usr/bin/env bash
# Runs the full backend gate the way CI does: clippy, then the WHOLE test suite against Atlas
# on a fresh throwaway database (never Docker Mongo, never the shared simplebash_pos_test).
# Without MONGODB_URI the Mongo-backed suites skip locally and only fail in CI.
# The URI is read from the workspace-root .env.docker.local and never printed.
set -euo pipefail

cd "$(dirname "$0")/.."
ENV_FILE="${ATLAS_ENV_FILE:-../../.env.docker.local}"

# sed (not a heredoc): macOS bash 3.2 cannot parse a heredoc with quotes inside $(...).
MONGODB_URI="$(sed -n -e 's/^[[:space:]]*MONGODB_URI=//p' "$ENV_FILE" | head -n1 | tr -d "\"'")"
if [ -z "$MONGODB_URI" ]; then
  echo "MONGODB_URI not found in $ENV_FILE" >&2
  exit 1
fi

DB="jtcheck_$(python3 -c 'import secrets; print(secrets.token_hex(10))')"
export MONGODB_URI MONGODB_DB_NAME="$DB" MONGODB_TEST_DB_NAME="$DB"

# Drop only the database this run created (name is generated above, always jtcheck_<hex>).
cleanup() {
  if command -v mongosh >/dev/null; then
    mongosh "$MONGODB_URI" --quiet --eval "db.getSiblingDB('$DB').dropDatabase()" >/dev/null 2>&1 \
      || echo "could not drop $DB; drop it by hand (Atlas caps at 500 collections)" >&2
  else
    echo "mongosh missing: drop $DB by hand (Atlas caps at 500 collections)" >&2
  fi
}
trap cleanup EXIT

echo "== clippy (CI flags) =="
cargo clippy --all-targets --all-features -- -D warnings

echo "== cargo test --no-fail-fast on Atlas db $DB =="
cargo test --no-fail-fast
