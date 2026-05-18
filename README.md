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

This repository currently initializes a Rust workspace with a single `server` crate:

```bash
cargo build
```
