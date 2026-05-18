#!/usr/bin/env bash
set -euo pipefail

export PATH="/root/.cargo/bin:/usr/local/cargo/bin:/usr/local/rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin:/usr/lib/postgresql/16/bin:$PATH"

DATA_DIR="/tmp/zeroclaw-e2e-pgdata"
SOCKET_DIR="/tmp/zeroclaw-e2e-pg"
LOG_FILE="/tmp/zeroclaw-e2e-pg.log"
PORT="55432"
DB_NAME="graph_playground_e2e"
SERVER_PID=""

cleanup() {
  if [[ -n "${SERVER_PID}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi

  if [[ -d "${DATA_DIR}" ]]; then
    runuser -u postgres -- pg_ctl -D "${DATA_DIR}" -m immediate stop >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT INT TERM

rm -rf "${DATA_DIR}" "${SOCKET_DIR}" "${LOG_FILE}"
install -d -m 0700 -o postgres -g postgres "${DATA_DIR}" "${SOCKET_DIR}"

runuser -u postgres -- initdb -D "${DATA_DIR}" --auth=trust --username=postgres >/dev/null
runuser -u postgres -- pg_ctl -D "${DATA_DIR}" -l "${LOG_FILE}" -o "-F -c listen_addresses='' -p ${PORT} -k ${SOCKET_DIR}" -w start >/dev/null

runuser -u postgres -- createdb -h "${SOCKET_DIR}" -p "${PORT}" -U postgres "${DB_NAME}"

export DATABASE_URL="postgresql://postgres@localhost/${DB_NAME}?host=${SOCKET_DIR}&port=${PORT}"

cd /workspace/web
npm run build

cargo run --manifest-path /workspace/server/Cargo.toml --bin seed

BIND_ADDR=127.0.0.1:8080 SEED_ON_START=true cargo run --manifest-path /workspace/server/Cargo.toml --bin server &
SERVER_PID="$!"
wait "${SERVER_PID}"
