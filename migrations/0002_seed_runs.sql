CREATE TABLE IF NOT EXISTS seed_runs (
    dataset TEXT NOT NULL,
    version INTEGER NOT NULL,
    node_count INTEGER NOT NULL,
    edge_count INTEGER NOT NULL,
    loaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (dataset, version)
);
