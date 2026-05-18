CREATE TABLE IF NOT EXISTS seed_runs (
    dataset TEXT NOT NULL,
    version INTEGER NOT NULL,
    node_count INTEGER NOT NULL,
    edge_count INTEGER NOT NULL,
    loaded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (dataset, version)
);
