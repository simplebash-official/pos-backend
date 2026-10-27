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

# Two runs share one Atlas cluster and one 500-collection quota, so a second copy fails the first
# (and its sweep below would drop the other run's databases). Refuse to start while one is live.
LOCK="${TMPDIR:-/tmp}/check-atlas.lock"
if ! mkdir "$LOCK" 2>/dev/null; then
  if kill -0 "$(cat "$LOCK/pid" 2>/dev/null)" 2>/dev/null; then
    echo "another check-atlas run is active (pid $(cat "$LOCK/pid")); wait for it or stop it first" >&2
    exit 1
  fi
  rm -rf "$LOCK" && mkdir "$LOCK"   # stale lock from a killed run
fi
echo $$ > "$LOCK/pid"
trap 'rm -rf "$LOCK"' EXIT

# Atlas (free tier) allows 500 collections in total and every throwaway test database holds ~35, so
# databases left behind by interrupted runs make the next run fail with "already using 500
# collections of 500" (it looks like a mass regression but is not). Sweep ONLY our own pattern
# jt<area>_<20-24 hex> (this repo) and jwa_<24 hex> (identity-server tests, same leak, same quota);
# simplebash_* and anything else are never touched.
if pgrep -f "backend/target/debug/deps" >/dev/null; then
  echo "another backend test run is active; skipping the leftover sweep" >&2
else
  mongosh "$MONGODB_URI" --quiet --eval '
    db.adminCommand({listDatabases:1,nameOnly:true}).databases.forEach(d => {
      if (/^(jt[a-z]+_[0-9a-f]{20,24}|jwa_[0-9a-f]{24})$/.test(d.name)) db.getSiblingDB(d.name).dropDatabase();
    })' >/dev/null 2>&1 || echo "leftover sweep failed (mongosh); continuing" >&2
fi

DB="jtcheck_$(python3 -c 'import secrets; print(secrets.token_hex(10))')"
export MONGODB_URI MONGODB_DB_NAME="$DB" MONGODB_TEST_DB_NAME="$DB"

# Drop only the database this run created (name is generated above, always jtcheck_<hex>).
cleanup() {
  rm -rf "$LOCK"
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
# Each parallel Mongo test holds a ~42-collection database, so unbounded threads (one per core)
# can alone pass the 500 quota. 4 threads keep a full run near 170 on top of the baseline.
RUST_TEST_THREADS="${CHECK_ATLAS_THREADS:-4}" cargo test --no-fail-fast
