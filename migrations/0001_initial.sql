CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    labels TEXT NOT NULL DEFAULT '[]',
    properties TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS edges (
    id TEXT PRIMARY KEY,
    start_id TEXT NOT NULL,
    end_id TEXT NOT NULL,
    type TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_nodes_created_at
    ON nodes (created_at, id);

CREATE INDEX IF NOT EXISTS idx_edges_start_id
    ON edges (start_id);

CREATE INDEX IF NOT EXISTS idx_edges_end_id
    ON edges (end_id);

CREATE INDEX IF NOT EXISTS idx_edges_type
    ON edges (type);
