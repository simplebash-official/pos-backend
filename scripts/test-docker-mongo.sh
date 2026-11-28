#!/usr/bin/env bash
# `cargo test --workspace` against a throwaway MongoDB in Docker, set up as in
# .github/workflows/ci.yml (a single-node replica set). The container is removed afterwards.
set -euo pipefail

cd "$(dirname "$0")/.."

NAME=pos-test-mongo
PORT=27027

docker info >/dev/null 2>&1 || {
  echo "Docker is not running: start Docker Desktop and retry." >&2
  exit 1
}

# `create` fails while the name is taken, so a second run cannot remove the first run's database.
docker create --name "$NAME" -p "127.0.0.1:$PORT:27017" mongo:7.0 \
  --replSet rs0 --bind_ip_all >/dev/null || {
  echo "If $NAME is left over from a killed run: docker rm -f -v $NAME" >&2
  exit 1
}
trap 'docker rm -f -v "$NAME" >/dev/null' EXIT
docker start "$NAME" >/dev/null

mongo_eval() { docker exec "$NAME" mongosh --quiet --eval "$1" 2>/dev/null; }
is_up() { mongo_eval "db.adminCommand('ping').ok" >/dev/null; }
is_primary() { [ "$(mongo_eval 'rs.status().myState')" = "1" ]; }

# Retries a check for about a minute.
wait_for() {
  for _ in $(seq 1 30); do
    "$@" && return 0
    sleep 2
  done
  echo "MongoDB did not become ready:" >&2
  docker logs --tail 20 "$NAME" >&2
  return 1
}

wait_for is_up
mongo_eval "rs.initiate({_id: 'rs0', members: [{_id: 0, host: 'localhost:27017'}]})" >/dev/null
wait_for is_primary

# Exported so they win over `.env`: dotenv never overrides a variable that is already set.
export MONGODB_URI="mongodb://127.0.0.1:$PORT/?directConnection=true"
export MONGODB_DB_NAME=jtlocal MONGODB_TEST_DB_NAME=jtlocal

cargo test --workspace
