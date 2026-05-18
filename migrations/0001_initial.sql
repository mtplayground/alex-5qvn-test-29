CREATE TABLE nodes (
    id UUID PRIMARY KEY,
    labels TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    properties JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE edges (
    id UUID PRIMARY KEY,
    start_id UUID NOT NULL,
    end_id UUID NOT NULL,
    type TEXT NOT NULL,
    properties JSONB NOT NULL DEFAULT '{}'::JSONB
);

CREATE INDEX idx_nodes_labels_gin
    ON nodes
    USING GIN (labels);

CREATE INDEX idx_nodes_properties_gin
    ON nodes
    USING GIN (properties);

CREATE INDEX idx_edges_start_id
    ON edges (start_id);

CREATE INDEX idx_edges_end_id
    ON edges (end_id);

CREATE INDEX idx_edges_type
    ON edges (type);

CREATE INDEX idx_edges_properties_gin
    ON edges
    USING GIN (properties);
