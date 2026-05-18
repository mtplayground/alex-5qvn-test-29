# alex-5qvn-test-29

Repository scaffold for a graph playground built with a Rust backend and a separate web frontend.

## Layout

- `server/`: Rust binary crate that will host the backend application
- `web/`: frontend workspace for the browser application
- `migrations/`: PostgreSQL schema migrations

## Environment

Copy `.env.example` into your local environment management flow and provide a valid PostgreSQL connection string.

- `DATABASE_URL`: PostgreSQL connection string
- `BIND_ADDR`: server bind address
- `SEED_ON_START`: whether seed loading should run at startup

## Build

This repository currently contains a Rust server crate and a Vite frontend.

```bash
cargo build
```

## Local frontend + server flow

For a production-style local run where the Rust binary serves the frontend bundle:

```bash
cd web
npm install
npm run build

cd ..
export DATABASE_URL=$(cat /workspace/.database_url)
cargo run --release
```

The server serves `web/dist/` at `/`, including client-side route fallback to `index.html`.
