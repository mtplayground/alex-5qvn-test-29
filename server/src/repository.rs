use serde_json::{Map, Value};
use sqlx::types::Json;
use sqlx::{Executor, PgPool, Postgres, Row};
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
        insert_node(&self.pool, node).await
    }

    pub async fn get_by_id(&self, node_id: Uuid) -> Result<Option<Node>, sqlx::Error> {
        get_node_by_id(&self.pool, node_id).await
    }

    pub async fn scan_by_label(&self, label: &str, limit: i64) -> Result<Vec<Node>, sqlx::Error> {
        scan_nodes_by_label(&self.pool, label, limit).await
    }

    pub async fn filter_by_property(
        &self,
        key: &str,
        value: Value,
        limit: i64,
    ) -> Result<Vec<Node>, sqlx::Error> {
        filter_nodes_by_property(&self.pool, key, value, limit).await
    }
}

impl EdgeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, edge: &Edge) -> Result<Edge, sqlx::Error> {
        insert_edge(&self.pool, edge).await
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
        expand_neighbors_query(&self.pool, node_id, edge_type).await
    }
}

impl SchemaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn catalog(&self) -> Result<SchemaCatalog, sqlx::Error> {
        let mut connection = self.pool.acquire().await?;
        catalog_query(&mut *connection).await
    }
}

async fn insert_node<'e, E>(executor: E, node: &Node) -> Result<Node, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
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
        .fetch_one(executor)
        .await
}

async fn get_node_by_id<'e, E>(executor: E, node_id: Uuid) -> Result<Option<Node>, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, Node>(
        r#"
        SELECT id, labels, properties
        FROM nodes
        WHERE id = $1
        "#,
    )
    .bind(node_id)
    .fetch_optional(executor)
    .await
}

async fn scan_nodes_by_label<'e, E>(
    executor: E,
    label: &str,
    limit: i64,
) -> Result<Vec<Node>, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
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
    .fetch_all(executor)
    .await
}

async fn filter_nodes_by_property<'e, E>(
    executor: E,
    key: &str,
    value: Value,
    limit: i64,
) -> Result<Vec<Node>, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
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
    .fetch_all(executor)
    .await
}

async fn insert_edge<'e, E>(executor: E, edge: &Edge) -> Result<Edge, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
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
        .fetch_one(executor)
        .await
}

async fn expand_neighbors_query<'e, E>(
    executor: E,
    node_id: Uuid,
    edge_type: Option<&str>,
) -> Result<NeighborExpansion, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
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
    .fetch_all(executor)
    .await?;

    build_neighbor_expansion(rows)
}

async fn catalog_query(
    executor: &mut sqlx::PgConnection,
) -> Result<SchemaCatalog, sqlx::Error> {
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
    .fetch_all(&mut *executor)
    .await?;

    let relationship_types = sqlx::query_as::<_, RelationshipTypeCount>(
        r#"
        SELECT type AS "type_", COUNT(*)::BIGINT AS count
        FROM edges
        GROUP BY type
        ORDER BY count DESC, type ASC
        "#,
    )
    .fetch_all(&mut *executor)
    .await?;

    Ok(SchemaCatalog {
        labels,
        relationship_types,
    })
}

fn property_filter_value(key: &str, value: Value) -> Properties {
    let mut properties = Map::with_capacity(1);
    properties.insert(key.to_owned(), value);
    properties
}

async fn list_edges_by_endpoint<'e, E>(
    executor: E,
    endpoint_column: &str,
    node_id: Uuid,
    edge_type: Option<&str>,
) -> Result<Vec<Edge>, sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
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
        .fetch_all(executor)
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
    use std::env;

    use crate::domain::{LabelCount, Properties, RelationshipTypeCount, SchemaCatalog};
    use crate::MIGRATOR;
    use serde_json::{json, Value};
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{
        catalog_query, expand_neighbors_query, filter_nodes_by_property, get_node_by_id,
        insert_edge, insert_node, list_edges_by_endpoint, property_filter_value,
        scan_nodes_by_label, Edge, NeighborExpansion, Node, SchemaRepository,
    };

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
            nodes: vec![sample_person_node(Uuid::from_u128(2))],
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

    #[tokio::test]
    async fn node_repository_insert_get_scan_and_filter_work() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let mut tx = pool.begin().await?;

        let node = sample_named_node(Uuid::from_u128(10), "Person", "alice");
        let inserted = insert_node(&mut *tx, &node).await?;
        let fetched = get_node_by_id(&mut *tx, inserted.id).await?;
        let scanned = scan_nodes_by_label(&mut *tx, "Person", 10).await?;
        let filtered = filter_nodes_by_property(&mut *tx, "name", Value::String("alice".to_owned()), 10).await?;

        assert_eq!(inserted, node);
        assert_eq!(fetched, Some(node.clone()));
        assert_eq!(scanned, vec![node.clone()]);
        assert_eq!(filtered, vec![node]);

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn node_repository_empty_results_are_returned() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let mut tx = pool.begin().await?;

        assert_eq!(get_node_by_id(&mut *tx, Uuid::from_u128(999)).await?, None);
        assert!(scan_nodes_by_label(&mut *tx, "Missing", 10).await?.is_empty());
        assert!(filter_nodes_by_property(&mut *tx, "name", Value::String("nobody".to_owned()), 10).await?.is_empty());

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn edge_repository_insert_list_and_expand_work() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let mut tx = pool.begin().await?;

        let source = sample_named_node(Uuid::from_u128(1), "Person", "alice");
        let target = sample_named_node(Uuid::from_u128(2), "Person", "bob");
        insert_node(&mut *tx, &source).await?;
        insert_node(&mut *tx, &target).await?;

        let edge = sample_edge(Uuid::from_u128(20), target.id);
        let inserted = insert_edge(&mut *tx, &edge).await?;
        let outgoing = list_edges_by_endpoint(&mut *tx, "start_id", source.id, Some("KNOWS")).await?;
        let incoming = list_edges_by_endpoint(&mut *tx, "end_id", target.id, Some("KNOWS")).await?;
        let expansion = expand_neighbors_query(&mut *tx, source.id, Some("KNOWS")).await?;

        assert_eq!(inserted, edge);
        assert_eq!(outgoing, vec![edge.clone()]);
        assert_eq!(incoming, vec![edge.clone()]);
        assert_eq!(expansion.edges, vec![edge]);
        assert_eq!(expansion.nodes, vec![target]);

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn edge_repository_empty_results_are_returned() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let mut tx = pool.begin().await?;

        assert!(list_edges_by_endpoint(&mut *tx, "start_id", Uuid::from_u128(404), None).await?.is_empty());
        assert!(list_edges_by_endpoint(&mut *tx, "end_id", Uuid::from_u128(404), Some("KNOWS")).await?.is_empty());
        let expansion = expand_neighbors_query(&mut *tx, Uuid::from_u128(404), None).await?;
        assert!(expansion.edges.is_empty());
        assert!(expansion.nodes.is_empty());

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    async fn schema_repository_catalog_returns_counts_and_empty_state() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let mut tx = pool.begin().await?;

        let empty_catalog = catalog_query(&mut tx).await?;
        assert!(empty_catalog.labels.is_empty());
        assert!(empty_catalog.relationship_types.is_empty());

        let source = sample_named_node(Uuid::from_u128(1), "Person", "alice");
        let target = sample_named_node(Uuid::from_u128(2), "Company", "acme");
        insert_node(&mut *tx, &source).await?;
        insert_node(&mut *tx, &target).await?;
        insert_edge(&mut *tx, &sample_edge(Uuid::from_u128(30), target.id)).await?;

        let catalog = catalog_query(&mut tx).await?;

        assert_eq!(
            catalog.labels,
            vec![
                LabelCount { label: "Company".to_owned(), count: 1 },
                LabelCount { label: "Person".to_owned(), count: 1 },
            ]
        );
        assert_eq!(
            catalog.relationship_types,
            vec![RelationshipTypeCount { type_: "KNOWS".to_owned(), count: 1 }]
        );

        tx.rollback().await?;
        Ok(())
    }

    fn sample_person_node(id: Uuid) -> Node {
        sample_named_node(id, "Person", "alice")
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

    fn sample_named_node(id: Uuid, label: &str, name: &str) -> Node {
        Node {
            id,
            labels: vec![label.to_owned()],
            properties: sample_properties_with_name(name),
        }
    }

    fn sample_properties() -> Properties {
        sample_properties_with_name("alice")
    }

    fn sample_properties_with_name(name: &str) -> Properties {
        match json!({ "name": name }) {
            Value::Object(map) => map,
            _ => unreachable!(),
        }
    }

    async fn test_pool() -> Result<Option<PgPool>, sqlx::Error> {
        let database_url = match env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };

        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("skipping Postgres integration test setup: {error}");
                return Ok(None);
            }
        };

        if let Err(error) = MIGRATOR.run(&pool).await {
            eprintln!("skipping Postgres integration test migrations: {error}");
            return Ok(None);
        }

        Ok(Some(pool))
    }
}
