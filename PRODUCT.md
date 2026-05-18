# Product Snapshot

## What this is

Transit-themed graph playground built on PostgreSQL, a Rust `axum` backend, and a Vite/React frontend. It lets a user run a constrained Cypher subset, inspect results as graph/table/JSON, and explore a seeded demo network visually.

## What it does today

- Serves a browser UI and JSON API from the Rust server
- Executes a limited Cypher subset over PostgreSQL
- Seeds an idempotent transit dataset on demand or at startup
- Renders graph results in Cytoscape with pan, zoom, drag, selection, and expand-on-double-click
- Exposes schema counts and per-node neighborhood expansion
- Includes a Playwright smoke test for the end-to-end flow

## User-facing features

- Query editor with CodeMirror highlighting and `Ctrl/Cmd+Enter` execution
- Result views for `Graph`, `Table`, and `Raw JSON` without rerunning the query
- Schema sidebar with one-click label scans
- Node inspector panel showing labels and properties
- Mutation queries (`CREATE`, supported `MERGE`) merge returned graph data into the active canvas
- Inline error display with API `error`, `line`, and `col` context

## Backend contract

- Routes:
  - `GET /healthz`
  - `POST /cypher`
  - `GET /schema`
  - `GET /node/:id`
- Static frontend is served from `web/dist` with SPA fallback
- All persistent state is PostgreSQL-backed via `sqlx`
- Error responses are JSON and consistent across parse, planner, executor, and repository failures

## Data model and seed conventions

- Core graph entities are `Node` and `Edge` with JSON properties
- Seed data lives in `seeds/transit-network.json`
- Current demo dataset: 345 nodes, 625 edges
- Seed loading is idempotent and guarded by both `SEED_ON_START` and a sentinel row

## Supported Cypher surface

- `MATCH`, `WHERE`, `RETURN`, `LIMIT`
- `CREATE`
- `MERGE` for a single labeled node with one unique key property
- Node patterns and relationship paths up to 2 hops
- Property maps, literals, identifiers, property access, and boolean/comparison expressions

## Conventions

- Frontend dev server runs on `:3000` and proxies API calls to the Rust server on `:8080`
- The Rust server expects `web/dist` to exist, even for local backend runs
- Release flow is `npm run build` in `web/` followed by `cargo build --release`, or `./scripts/release-build.sh`
