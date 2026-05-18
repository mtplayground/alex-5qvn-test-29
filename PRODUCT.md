# Product Snapshot

## What this is

Transit-themed graph playground with a Rust `axum` server and a Vite/React frontend. The app lets a user run a constrained Cypher subset, inspect schema and node neighborhoods, and explore results as a graph, table, or raw JSON.

## What it does today

- Serves the built SPA and JSON API from one Rust binary
- Persists graph data locally in SQLite at `/data/graph.db`
- Runs SQLx migrations on startup and can seed the demo network automatically
- Exposes schema counts, node-neighbor expansion, and Cypher execution over HTTP
- Mounts the Cytoscape workspace as the default root experience
- Auto-loads an initial sample graph on first page load
- Renders query results in a browser workspace with editor, schema sidebar, graph canvas, and detail panels

## User-facing features

- Default landing view is the graph workspace rather than a separate splash page
- CodeMirror-based Cypher editor with keyboard submit
- Result tabs for graph, table, and raw JSON views
- Schema sidebar with one-click label scans
- Interactive Cytoscape canvas for graph exploration
- Node inspection and neighborhood expansion
- Inline API error display with structured error details

## Backend contract

- Routes:
  - `GET /healthz`
  - `POST /cypher`
  - `GET /schema`
  - `GET /node/:id`
- Static frontend is served from `web/dist` with SPA fallback
- Startup flow is: open SQLite DB, run migrations, optionally seed, then serve HTTP
- Runtime config is driven by `BIND_ADDR`, `SEED_ON_START`, and optional `DATA_DIR`

## Data and seed conventions

- Core entities are `Node` and `Edge` with JSON properties
- Persistent state lives in SQLite, not Postgres
- Seed data lives in `seeds/transit-network.json`
- Current demo dataset is 345 nodes and 625 edges
- Seed loading is idempotent and guarded by `SEED_ON_START` plus a `seed_runs` sentinel

## Supported Cypher surface

- `MATCH`, `WHERE`, `RETURN`, `LIMIT`
- `CREATE`
- `MERGE` for a single labeled node with one unique key property
- Node patterns and relationship paths up to 2 hops
- Property maps, literals, identifiers, property access, and boolean/comparison expressions

## Conventions

- Frontend development runs on `:3000`; the backend serves the production bundle on `:8080`
- The backend expects `web/dist` to exist before serving the UI
- The default startup query is `MATCH (n) RETURN n LIMIT 25` and it is executed through the same frontend query path as manual runs
- Release flow is frontend build first, then Rust release build
- Production deployment is expected to use durable filesystem storage at `/data`
