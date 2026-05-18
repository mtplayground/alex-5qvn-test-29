# Web

Frontend scaffold for the graph playground.

## Stack

- Vite
- React + TypeScript
- Tailwind CSS
- shadcn/ui-compatible component setup

## Scripts

- `npm run dev`: starts Vite on `0.0.0.0:3000`
- `npm run build`: type-checks and builds the production bundle

## Proxy

The Vite dev server proxies backend requests to the Rust server on `http://127.0.0.1:8080` for:

- `/healthz`
- `/cypher`
- `/schema`
- `/node`
