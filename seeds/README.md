# Seed Datasets

`transit-network.json` contains a symbolic graph fixture for later loader work.

- Node and edge `id` values are stable symbolic keys, not database UUIDs.
- Each node record uses `{ id, labels, properties }`.
- Each edge record uses `{ id, start_id, end_id, type, properties }`.
- Current dataset size: 345 nodes, 625 edges.
