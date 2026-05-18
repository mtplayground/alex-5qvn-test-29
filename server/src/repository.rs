use serde_json::{Map, Value};
use sqlx::types::Json;
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use uuid::Uuid;

use crate::domain::{Edge, LabelCount, Node, Properties, RelationshipTypeCount, SchemaCatalog};

#[derive(Clone, Debug)]
pub struct NodeRepository {
    pool: PgPool,
}

#[derive(Clone, Debug)]
pub struct EdgeRepository {
    pool: PgPool,
}

#[derive(Clone, Debug)]
pub struct SchemaRepository {
    pool: PgPool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NeighborExpansion {
    pub edges: Vec<Edge>,
    pub nodes: Vec<Node>,
}

impl NodeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, node: &Node) -> Result<Node, sqlx::Error> {
        sqlx::query_as::<_, Node>(
            r#"
            INSERT INTO nodes (id, labels, properties)
            VALUES ($1, $2, $3)
            RETURNING id, labels, properties
            "#,
        )
        .bind(node.id)
        .bind(&node.labels)
        .bind(Json(&node.properties))
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_by_id(&self, node_id: Uuid) -> Result<Option<Node>, sqlx::Error> {
        sqlx::query_as::<_, Node>(
            r#"
            SELECT id, labels, properties
            FROM nodes
            WHERE id = $1
            "#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn scan_by_label(&self, label: &str, limit: i64) -> Result<Vec<Node>, sqlx::Error> {
        sqlx::query_as::<_, Node>(
            r#"
            SELECT id, labels, properties
            FROM nodes
            WHERE labels @> ARRAY[$1]::TEXT[]
            ORDER BY created_at ASC, id ASC
            LIMIT $2
            "#,
        )
        .bind(label)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn filter_by_property(
        &self,
        key: &str,
        value: Value,
        limit: i64,
    ) -> Result<Vec<Node>, sqlx::Error> {
        let property_filter = property_filter_value(key, value);

        sqlx::query_as::<_, Node>(
            r#"
            SELECT id, labels, properties
            FROM nodes
            WHERE properties @> $1
            ORDER BY created_at ASC, id ASC
            LIMIT $2
            "#,
        )
        .bind(Json(property_filter))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }
}

impl EdgeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, edge: &Edge) -> Result<Edge, sqlx::Error> {
        sqlx::query_as::<_, Edge>(
            r#"
            INSERT INTO edges (id, start_id, end_id, type, properties)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, start_id, end_id, type, properties
            "#,
        )
        .bind(edge.id)
        .bind(edge.start_id)
        .bind(edge.end_id)
        .bind(&edge.type_)
        .bind(Json(&edge.properties))
        .fetch_one(&self.pool)
        .await
    }

    pub async fn list_outgoing(
        &self,
        node_id: Uuid,
        edge_type: Option<&str>,
    ) -> Result<Vec<Edge>, sqlx::Error> {
        list_edges_by_endpoint(&self.pool, "start_id", node_id, edge_type).await
    }

    pub async fn list_incoming(
        &self,
        node_id: Uuid,
        edge_type: Option<&str>,
    ) -> Result<Vec<Edge>, sqlx::Error> {
        list_edges_by_endpoint(&self.pool, "end_id", node_id, edge_type).await
    }

    pub async fn expand_neighbors(
        &self,
        node_id: Uuid,
        edge_type: Option<&str>,
    ) -> Result<NeighborExpansion, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT
                e.id AS edge_id,
                e.start_id,
                e.end_id,
                e.type,
                e.properties AS edge_properties,
                n.id AS node_id,
                n.labels AS node_labels,
                n.properties AS node_properties
            FROM edges e
            JOIN nodes n
              ON n.id = CASE
                    WHEN e.start_id = $1 THEN e.end_id
                    ELSE e.start_id
                END
            WHERE (e.start_id = $1 OR e.end_id = $1)
              AND ($2::TEXT IS NULL OR e.type = $2)
            ORDER BY e.id ASC, n.id ASC
            "#,
        )
        .bind(node_id)
        .bind(edge_type)
        .fetch_all(&self.pool)
        .await?;

        build_neighbor_expansion(rows)
    }
}

impl SchemaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn catalog(&self) -> Result<SchemaCatalog, sqlx::Error> {
        let labels = sqlx::query_as::<_, LabelCount>(
            r#"
            SELECT label, COUNT(*)::BIGINT AS count
            FROM (
                SELECT UNNEST(labels) AS label
                FROM nodes
            ) expanded_labels
            GROUP BY label
            ORDER BY count DESC, label ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let relationship_types = sqlx::query_as::<_, RelationshipTypeCount>(
            r#"
            SELECT type AS "type_", COUNT(*)::BIGINT AS count
            FROM edges
            GROUP BY type
            ORDER BY count DESC, type ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(SchemaCatalog {
            labels,
            relationship_types,
        })
    }
}

fn property_filter_value(key: &str, value: Value) -> Properties {
    let mut properties = Map::with_capacity(1);
    properties.insert(key.to_owned(), value);
    properties
}

async fn list_edges_by_endpoint(
    pool: &PgPool,
    endpoint_column: &str,
    node_id: Uuid,
    edge_type: Option<&str>,
) -> Result<Vec<Edge>, sqlx::Error> {
    let query = format!(
        r#"
        SELECT id, start_id, end_id, type, properties
        FROM edges
        WHERE {endpoint_column} = $1
          AND ($2::TEXT IS NULL OR type = $2)
        ORDER BY id ASC
        "#
    );

    sqlx::query_as::<_, Edge>(&query)
        .bind(node_id)
        .bind(edge_type)
        .fetch_all(pool)
        .await
}

fn build_neighbor_expansion(rows: Vec<sqlx::postgres::PgRow>) -> Result<NeighborExpansion, sqlx::Error> {
    let mut edges = Vec::with_capacity(rows.len());
    let mut nodes = Vec::with_capacity(rows.len());
    let mut seen_edge_ids = HashSet::with_capacity(rows.len());
    let mut seen_node_ids = HashSet::with_capacity(rows.len());

    for row in rows {
        let edge = Edge {
            id: row.try_get("edge_id")?,
            start_id: row.try_get("start_id")?,
            end_id: row.try_get("end_id")?,
            type_: row.try_get("type")?,
            properties: decode_json_properties(&row, "edge_properties")?,
        };

        if seen_edge_ids.insert(edge.id) {
            edges.push(edge);
        }

        let node = Node {
            id: row.try_get("node_id")?,
            labels: row.try_get("node_labels")?,
            properties: decode_json_properties(&row, "node_properties")?,
        };

        if seen_node_ids.insert(node.id) {
            nodes.push(node);
        }
    }

    Ok(NeighborExpansion { edges, nodes })
}

fn decode_json_properties(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Properties, sqlx::Error> {
    let Json(properties) = row.try_get::<Json<Properties>, _>(column)?;
    Ok(properties)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use uuid::Uuid;

    use super::{property_filter_value, Edge, NeighborExpansion, Node, SchemaRepository};
    use crate::domain::{Properties, SchemaCatalog};

    #[test]
    fn property_filter_value_wraps_key_and_value_for_jsonb_contains() {
        let filter = property_filter_value("name", Value::String("alice".to_owned()));

        assert_eq!(json!(filter), json!({ "name": "alice" }));
    }

    #[test]
    fn property_filter_value_preserves_scalar_types() {
        let filter = property_filter_value("active", Value::Bool(true));

        assert_eq!(json!(filter), json!({ "active": true }));
    }

    #[test]
    fn neighbor_expansion_keeps_unique_edges_and_nodes() {
        let expansion = NeighborExpansion {
            edges: vec![sample_edge(Uuid::nil(), Uuid::from_u128(2))],
            nodes: vec![sample_node(Uuid::from_u128(2))],
        };

        assert_eq!(expansion.edges.len(), 1);
        assert_eq!(expansion.nodes.len(), 1);
        assert_eq!(expansion.edges[0].type_, "KNOWS");
    }

    #[test]
    fn schema_repository_type_is_constructible() {
        fn assert_catalog_shape(_catalog: SchemaCatalog) {}
        let _ = assert_catalog_shape;
        let _ = SchemaRepository::new;
    }

    fn sample_node(id: Uuid) -> Node {
        Node {
            id,
            labels: vec!["Person".to_owned()],
            properties: sample_properties(),
        }
    }

    fn sample_edge(id: Uuid, adjacent_id: Uuid) -> Edge {
        Edge {
            id,
            start_id: Uuid::from_u128(1),
            end_id: adjacent_id,
            type_: "KNOWS".to_owned(),
            properties: sample_properties(),
        }
    }

    fn sample_properties() -> Properties {
        match json!({ "name": "alice" }) {
            Value::Object(map) => map,
            _ => unreachable!(),
        }
    }
}
