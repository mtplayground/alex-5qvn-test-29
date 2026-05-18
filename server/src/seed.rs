use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value};
use sqlx::types::Json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::{Edge, Node, Properties};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedStatus {
    SkippedDisabled,
    SkippedSentinel,
    Loaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedReport {
    pub status: SeedStatus,
    pub datasets: usize,
    pub nodes_created: usize,
    pub nodes_matched: usize,
    pub edges_created: usize,
    pub edges_matched: usize,
}

impl SeedReport {
    fn skipped_disabled() -> Self {
        Self {
            status: SeedStatus::SkippedDisabled,
            datasets: 0,
            nodes_created: 0,
            nodes_matched: 0,
            edges_created: 0,
            edges_matched: 0,
        }
    }

    fn skipped_sentinel() -> Self {
        Self {
            status: SeedStatus::SkippedSentinel,
            datasets: 0,
            nodes_created: 0,
            nodes_matched: 0,
            edges_created: 0,
            edges_matched: 0,
        }
    }

    fn loaded() -> Self {
        Self {
            status: SeedStatus::Loaded,
            datasets: 0,
            nodes_created: 0,
            nodes_matched: 0,
            edges_created: 0,
            edges_matched: 0,
        }
    }
}

#[derive(Debug)]
pub enum SeedError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Sqlx(sqlx::Error),
    MissingKeyProperty {
        label: String,
        key_property: &'static str,
    },
    MissingNodeReference(String),
    InvalidRecord(String),
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "seed I/O error: {error}"),
            Self::Json(error) => write!(f, "seed JSON error: {error}"),
            Self::Sqlx(error) => write!(f, "seed database error: {error}"),
            Self::MissingKeyProperty {
                label,
                key_property,
            } => write!(
                f,
                "seed node with label `{label}` is missing required key property `{key_property}`"
            ),
            Self::MissingNodeReference(seed_id) => {
                write!(f, "seed edge references unknown node `{seed_id}`")
            }
            Self::InvalidRecord(message) => write!(f, "invalid seed record: {message}"),
        }
    }
}

impl std::error::Error for SeedError {}

impl From<std::io::Error> for SeedError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for SeedError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<sqlx::Error> for SeedError {
    fn from(error: sqlx::Error) -> Self {
        Self::Sqlx(error)
    }
}

#[derive(Debug, Deserialize)]
struct SeedDataset {
    dataset: String,
    version: i32,
    nodes: Vec<SeedNodeRecord>,
    edges: Vec<SeedEdgeRecord>,
}

#[derive(Debug, Deserialize)]
struct SeedNodeRecord {
    id: String,
    labels: Vec<String>,
    properties: Properties,
}

#[derive(Debug, Deserialize)]
struct SeedEdgeRecord {
    id: String,
    start_id: String,
    end_id: String,
    #[serde(rename = "type")]
    type_: String,
    properties: Properties,
}

pub async fn load_seed_data(pool: &PgPool, seed_on_start: bool) -> Result<SeedReport, SeedError> {
    if !seed_on_start {
        return Ok(SeedReport::skipped_disabled());
    }

    load_seed_data_from_dir(pool, &default_seed_dir()).await
}

pub async fn load_seed_data_from_dir(pool: &PgPool, seed_dir: &Path) -> Result<SeedReport, SeedError> {
    if sentinel_exists(pool).await? {
        return Ok(SeedReport::skipped_sentinel());
    }

    let dataset_paths = dataset_paths(seed_dir)?;
    let mut transaction = pool.begin().await?;
    let mut report = SeedReport::loaded();

    for path in dataset_paths {
        let dataset = read_dataset(&path)?;
        report.datasets += 1;
        upsert_dataset(&mut transaction, &dataset, &mut report).await?;
        insert_seed_run(&mut transaction, &dataset).await?;
    }

    transaction.commit().await?;
    Ok(report)
}

fn default_seed_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("server crate should live under repo root")
        .join("seeds")
}

fn dataset_paths(seed_dir: &Path) -> Result<Vec<PathBuf>, SeedError> {
    let mut paths = fs::read_dir(seed_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn read_dataset(path: &Path) -> Result<SeedDataset, SeedError> {
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(SeedError::from)
}

async fn sentinel_exists(pool: &PgPool) -> Result<bool, SeedError> {
    let sentinel_exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM seed_runs)")
        .fetch_one(pool)
        .await?;
    Ok(sentinel_exists)
}

async fn upsert_dataset(
    transaction: &mut Transaction<'_, Postgres>,
    dataset: &SeedDataset,
    report: &mut SeedReport,
) -> Result<(), SeedError> {
    let mut node_id_map = std::collections::HashMap::with_capacity(dataset.nodes.len());

    for node in &dataset.nodes {
        let node_id = upsert_node(transaction, node, report).await?;
        node_id_map.insert(node.id.clone(), node_id);
    }

    for edge in &dataset.edges {
        upsert_edge(transaction, edge, &node_id_map, report).await?;
    }

    Ok(())
}

async fn upsert_node(
    transaction: &mut Transaction<'_, Postgres>,
    node: &SeedNodeRecord,
    report: &mut SeedReport,
) -> Result<Uuid, SeedError> {
    let label = primary_label(node)?;
    let key_property = key_property_for_label(label)?;
    let key_value = node
        .properties
        .get(key_property)
        .cloned()
        .ok_or_else(|| SeedError::MissingKeyProperty {
            label: label.to_owned(),
            key_property,
        })?;

    let property_filter = property_filter_value(key_property, key_value);
    let existing = sqlx::query_as::<_, Node>(
        r#"
        SELECT id, labels, properties
        FROM nodes
        WHERE labels @> ARRAY[$1]::TEXT[]
          AND properties @> $2
        ORDER BY created_at ASC, id ASC
        LIMIT 1
        "#,
    )
    .bind(label)
    .bind(Json(property_filter))
    .fetch_optional(transaction.as_mut())
    .await?;

    if let Some(node) = existing {
        report.nodes_matched += 1;
        return Ok(node.id);
    }

    let mut properties = node.properties.clone();
    properties.insert("seed_id".to_owned(), Value::String(node.id.clone()));

    let inserted = sqlx::query_as::<_, Node>(
        r#"
        INSERT INTO nodes (id, labels, properties)
        VALUES ($1, $2, $3)
        RETURNING id, labels, properties
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&node.labels)
    .bind(Json(properties))
    .fetch_one(transaction.as_mut())
    .await?;

    report.nodes_created += 1;
    Ok(inserted.id)
}

async fn upsert_edge(
    transaction: &mut Transaction<'_, Postgres>,
    edge: &SeedEdgeRecord,
    node_id_map: &std::collections::HashMap<String, Uuid>,
    report: &mut SeedReport,
) -> Result<(), SeedError> {
    let start_id = *node_id_map
        .get(&edge.start_id)
        .ok_or_else(|| SeedError::MissingNodeReference(edge.start_id.clone()))?;
    let end_id = *node_id_map
        .get(&edge.end_id)
        .ok_or_else(|| SeedError::MissingNodeReference(edge.end_id.clone()))?;

    let existing = sqlx::query_as::<_, Edge>(
        r#"
        SELECT id, start_id, end_id, type, properties
        FROM edges
        WHERE start_id = $1
          AND end_id = $2
          AND type = $3
        ORDER BY id ASC
        LIMIT 1
        "#,
    )
    .bind(start_id)
    .bind(end_id)
    .bind(&edge.type_)
    .fetch_optional(transaction.as_mut())
    .await?;

    if existing.is_some() {
        report.edges_matched += 1;
        return Ok(());
    }

    let mut properties = edge.properties.clone();
    properties.insert("seed_id".to_owned(), Value::String(edge.id.clone()));

    sqlx::query_as::<_, Edge>(
        r#"
        INSERT INTO edges (id, start_id, end_id, type, properties)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, start_id, end_id, type, properties
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(start_id)
    .bind(end_id)
    .bind(&edge.type_)
    .bind(Json(properties))
    .fetch_one(transaction.as_mut())
    .await?;

    report.edges_created += 1;
    Ok(())
}

async fn insert_seed_run(
    transaction: &mut Transaction<'_, Postgres>,
    dataset: &SeedDataset,
) -> Result<(), SeedError> {
    let node_count = i32::try_from(dataset.nodes.len()).map_err(|_| {
        SeedError::InvalidRecord(format!("dataset `{}` has too many nodes", dataset.dataset))
    })?;
    let edge_count = i32::try_from(dataset.edges.len()).map_err(|_| {
        SeedError::InvalidRecord(format!("dataset `{}` has too many edges", dataset.dataset))
    })?;

    sqlx::query(
        r#"
        INSERT INTO seed_runs (dataset, version, node_count, edge_count)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(&dataset.dataset)
    .bind(dataset.version)
    .bind(node_count)
    .bind(edge_count)
    .execute(transaction.as_mut())
    .await?;

    Ok(())
}

fn primary_label(node: &SeedNodeRecord) -> Result<&str, SeedError> {
    node.labels
        .first()
        .map(String::as_str)
        .ok_or_else(|| SeedError::InvalidRecord(format!("node `{}` has no labels", node.id)))
}

fn key_property_for_label(label: &str) -> Result<&'static str, SeedError> {
    match label {
        "Country" => Ok("code"),
        "City" | "Manufacturer" | "Operator" | "RollingStockModel" | "Line" | "Station" => {
            Ok("slug")
        }
        "Year" => Ok("value"),
        other => Err(SeedError::InvalidRecord(format!(
            "no seed key property configured for label `{other}`"
        ))),
    }
}

fn property_filter_value(key: &str, value: Value) -> Map<String, Value> {
    let mut property_filter = Map::new();
    property_filter.insert(key.to_owned(), value);
    property_filter
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;

    use super::{load_seed_data, load_seed_data_from_dir, SeedStatus};
    use crate::MIGRATOR;

    #[tokio::test]
    async fn loader_skips_when_disabled() -> Result<(), Box<dyn std::error::Error>> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };

        let report = load_seed_data(&pool, false).await?;

        assert_eq!(report.status, SeedStatus::SkippedDisabled);
        assert_eq!(report.datasets, 0);
        Ok(())
    }

    #[tokio::test]
    async fn loader_loads_fixture_and_records_sentinel() -> Result<(), Box<dyn std::error::Error>> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture_dir("seed-load")?;

        let report = load_seed_data_from_dir(&pool, &fixture).await?;

        assert_eq!(report.status, SeedStatus::Loaded);
        assert_eq!(report.datasets, 1);
        assert_eq!(report.nodes_created, 3);
        assert_eq!(report.nodes_matched, 0);
        assert_eq!(report.edges_created, 2);
        assert_eq!(report.edges_matched, 0);
        assert_eq!(count_rows(&pool, "nodes").await?, 3);
        assert_eq!(count_rows(&pool, "edges").await?, 2);
        assert_eq!(count_rows(&pool, "seed_runs").await?, 1);

        Ok(())
    }

    #[tokio::test]
    async fn loader_rerun_skips_when_sentinel_exists() -> Result<(), Box<dyn std::error::Error>> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture_dir("seed-rerun")?;

        let first = load_seed_data_from_dir(&pool, &fixture).await?;
        let second = load_seed_data_from_dir(&pool, &fixture).await?;

        assert_eq!(first.status, SeedStatus::Loaded);
        assert_eq!(second.status, SeedStatus::SkippedSentinel);
        assert_eq!(count_rows(&pool, "nodes").await?, 3);
        assert_eq!(count_rows(&pool, "edges").await?, 2);
        assert_eq!(count_rows(&pool, "seed_runs").await?, 1);

        Ok(())
    }

    async fn test_pool() -> Result<Option<PgPool>, sqlx::Error> {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };

        let schema_name = format!(
            "seed_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(1)
        );

        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("skipping seed loader test setup: {error}");
                return Ok(None);
            }
        };

        sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, schema_name))
            .execute(&pool)
            .await?;
        sqlx::query(&format!(r#"SET search_path TO "{}""#, schema_name))
            .execute(&pool)
            .await?;

        if let Err(error) = MIGRATOR.run(&pool).await {
            eprintln!("skipping seed loader migrations: {error}");
            return Ok(None);
        }

        Ok(Some(pool))
    }

    async fn count_rows(pool: &PgPool, table: &str) -> Result<i64, sqlx::Error> {
        let query = format!("SELECT COUNT(*)::BIGINT FROM {table}");
        sqlx::query_scalar::<_, i64>(&query).fetch_one(pool).await
    }

    fn fixture_dir(prefix: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir = std::env::temp_dir().join(format!("zeroclaw-{prefix}-{nanos}"));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("fixture.json"), fixture_json(prefix, nanos))?;
        Ok(dir)
    }

    fn fixture_json(prefix: &str, suffix: u128) -> String {
        let city_slug = format!("{prefix}-city-{suffix}");
        let country_code = format!("X{}", suffix % 90 + 10);
        let operator_slug = format!("{prefix}-operator-{suffix}");

        json!({
            "dataset": format!("{prefix}-{suffix}"),
            "version": 1,
            "nodes": [
                {
                    "id": "country",
                    "labels": ["Country"],
                    "properties": {
                        "name": format!("Country {suffix}"),
                        "code": country_code,
                        "region": "Test"
                    }
                },
                {
                    "id": "city",
                    "labels": ["City"],
                    "properties": {
                        "name": format!("City {suffix}"),
                        "slug": city_slug,
                        "population_millions": 1.2
                    }
                },
                {
                    "id": "operator",
                    "labels": ["Operator"],
                    "properties": {
                        "name": format!("Operator {suffix}"),
                        "slug": operator_slug,
                        "mode": "metro"
                    }
                }
            ],
            "edges": [
                {
                    "id": "city-country",
                    "start_id": "city",
                    "end_id": "country",
                    "type": "LOCATED_IN",
                    "properties": {
                        "kind": "administrative"
                    }
                },
                {
                    "id": "operator-city",
                    "start_id": "operator",
                    "end_id": "city",
                    "type": "SERVES",
                    "properties": {
                        "primary": true
                    }
                }
            ]
        })
        .to_string()
    }
}
