# alex-5qvn-test-29

Graph playground backed by PostgreSQL, a Rust `axum` API, and a Vite/React frontend. The server parses and executes a constrained Cypher subset, serves the built frontend from `web/dist`, and can seed a transit-focused demo graph on startup.

## Prerequisites

- Rust toolchain with `cargo`
- Node.js 20+ and `npm`
- PostgreSQL 16+ reachable through `DATABASE_URL`

## Repository layout

- `server/`: Rust backend, query parser/planner/executor, seed loader, and binaries
- `web/`: Vite + React + TypeScript frontend
- `migrations/`: SQLx migrations for the graph schema and seed sentinel table
- `seeds/`: transit-network seed dataset and format notes
- `scripts/`: release/build helpers

## Environment

Copy `.env.example` into your local env management flow and set the variables before running the app:

- `DATABASE_URL`: PostgreSQL connection string used by the server, tests, and seed binary
- `BIND_ADDR`: bind address for the Rust server, typically `0.0.0.0:8080`
- `SEED_ON_START`: `true` to load seed data during server startup, `false` to skip it

Example:

```bash
cp .env.example .env
export $(grep -v '^#' .env | xargs)
```

## Development workflow

Install frontend dependencies once:

```bash
cd web
npm install
```

Because the Rust server serves static assets from `web/dist`, build the frontend at least once before the first backend boot:

```bash
cd /workspace/web
npm run build
```

Run the backend:

```bash
cd /workspace
export DATABASE_URL=postgresql://postgres:postgres@localhost:5432/graph_playground
export BIND_ADDR=0.0.0.0:8080
export SEED_ON_START=true
cargo run --bin server
```

Run the Vite dev server in a second shell:

```bash
cd /workspace/web
npm run dev
```

Notes:

- Vite listens on `0.0.0.0:3000` and proxies API traffic to the Rust server on `:8080`.
- The Rust server still requires `web/dist` to exist, even if you are actively using `npm run dev`.
- If you want to seed without starting the server, run `cargo run --bin seed`.

## Release flow

The production build flow is:

```bash
cd /workspace/web
npm run build

cd /workspace
cargo build --release
```

The repo also includes a helper script that performs the same flow:

```bash
./scripts/release-build.sh
```

That script builds the frontend bundle first, then builds the Rust release binaries.

## Serving model

The Rust binary serves:

- `GET /healthz`
- `POST /cypher`
- `GET /schema`
- `GET /node/:id`
- `web/dist/` at `/`, with SPA fallback to `index.html`

## Seed data

`seeds/transit-network.json` contains a transit-focused graph fixture with stable symbolic IDs:

- 345 nodes
- 625 edges
- labels: `City`, `Country`, `Line`, `Manufacturer`, `Operator`, `RollingStockModel`, `Station`, `Year`
- relationship types: `BUILT_BY`, `CONNECTS_TO`, `HEADQUARTERED_IN`, `INTERCHANGE_WITH`, `LOCATED_IN`, `OPENED_IN`, `OPERATES`, `OWNED_BY`, `PART_OF`, `SERVES`, `TERMINUS_OF`, `USES_ROLLING_STOCK`

The loader is idempotent. It uses label-specific natural keys for node upserts, skips entirely when `SEED_ON_START=false`, and also skips if the seed sentinel row already exists.

## Supported Cypher subset

The parser/planner/executor intentionally supports a limited Cypher surface:

- `MATCH`, `WHERE`, `RETURN`, `LIMIT`
- `CREATE`
- `MERGE` for a single labeled node with one unique key property
- node patterns and relationship paths up to 2 hops
- property maps, literals, identifiers, property access
- boolean and comparison operators in `WHERE`

Examples:

```cypher
MATCH (n:Station) RETURN n LIMIT 25
MATCH (n:Station)-[r:LOCATED_IN]->(m:City) RETURN n, r, m
CREATE (n:Station {name: "TestStop"})
MERGE (n:Operator {name: "Metro Transit"}) RETURN n
```

Unsupported queries should return a clean JSON error payload rather than crashing the server or frontend.

## Validation

Common local checks:

```bash
cargo test
cargo build

cd web
npm run build
```

For browser smoke coverage:

```bash
cd /workspace/web
npm run test:e2e
```
