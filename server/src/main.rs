use std::env;
use std::error::Error;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use server::domain::SchemaCatalog;
use server::executor::QueryResult;
use server::parser::parse_ast;
use server::planner::plan_query;
use server::repository::SchemaRepository;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::services::{ServeDir, ServeFile};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Clone, Debug)]
struct AppConfig {
    bind_addr: SocketAddr,
    database_url: String,
    seed_on_start: bool,
    web_dist_dir: PathBuf,
}

impl AppConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let bind_addr = parse_bind_addr(&read_env("BIND_ADDR")?)?;
        let database_url = read_env("DATABASE_URL")?;
        let seed_on_start = parse_bool_env(&read_env("SEED_ON_START")?)?;
        let web_dist_dir = resolve_web_dist_dir()?;

        Ok(Self {
            bind_addr,
            database_url,
            seed_on_start,
            web_dist_dir,
        })
    }
}

#[derive(Clone, Debug)]
struct AppState {
    config: AppConfig,
    db_pool: PgPool,
}

#[derive(Debug)]
enum ConfigError {
    MissingVar(&'static str),
    InvalidBindAddr(std::net::AddrParseError),
    InvalidBool {
        name: &'static str,
        value: String,
    },
    MissingWebDist(PathBuf),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVar(name) => write!(f, "missing required environment variable `{name}`"),
            Self::InvalidBindAddr(error) => write!(f, "invalid BIND_ADDR value: {error}"),
            Self::InvalidBool { name, value } => {
                write!(f, "invalid boolean value for `{name}`: `{value}`")
            }
            Self::MissingWebDist(path) => {
                write!(
                    f,
                    "frontend build output not found at `{}`; run `npm run build` in `web/` first",
                    path.display()
                )
            }
        }
    }
}

impl Error for ConfigError {}

#[derive(Debug, Serialize)]
struct HealthResponse<'a> {
    status: &'a str,
}

#[derive(Debug, Deserialize)]
struct CypherRequest {
    query: String,
    params: Option<JsonValue>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    line: Option<usize>,
    col: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    let db_pool = create_db_pool(&config.database_url).await?;
    check_db_readiness(&db_pool).await?;
    let app = app_router(config.clone(), db_pool.clone());
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;

    info!(
        bind_addr = %config.bind_addr,
        seed_on_start = config.seed_on_start,
        database_ready = true,
        web_dist_dir = %config.web_dist_dir.display(),
        "starting server",
    );

    axum::serve(listener, app).await?;

    Ok(())
}

fn app_router(config: AppConfig, db_pool: PgPool) -> Router {
    let web_dist_dir = config.web_dist_dir.clone();

    Router::new()
        .route("/healthz", get(healthz))
        .route("/schema", get(schema))
        .route("/cypher", post(cypher))
        .fallback_service(static_assets_service(&web_dist_dir))
        .with_state(AppState { config, db_pool })
}

async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse<'static>>) {
    let _ = &state.config;
    let _ = &state.db_pool;
    (
        StatusCode::OK,
        Json(HealthResponse { status: "ok" }),
    )
}

async fn schema(
    State(state): State<AppState>,
) -> Result<Json<SchemaCatalog>, (StatusCode, Json<ErrorResponse>)> {
    execute_schema_request(&state.db_pool).await.map(Json)
}

async fn cypher(
    State(state): State<AppState>,
    Json(request): Json<CypherRequest>,
) -> Result<Json<QueryResult>, (StatusCode, Json<ErrorResponse>)> {
    execute_cypher_request(&state.db_pool, request)
        .await
        .map(Json)
}

async fn execute_cypher_request(
    db_pool: &PgPool,
    request: CypherRequest,
) -> Result<QueryResult, (StatusCode, Json<ErrorResponse>)> {
    let _ = &request.params;

    let ast = parse_ast(&request.query).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: error.message,
                line: Some(error.line),
                col: Some(error.column),
            }),
        )
    })?;

    let plan = plan_query(&ast).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: error.message,
                line: None,
                col: None,
            }),
        )
    })?;

    server::executor::execute_plan(db_pool.clone(), &plan)
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: error.message,
                    line: None,
                    col: None,
                }),
            )
        })
}

async fn execute_schema_request(
    db_pool: &PgPool,
) -> Result<SchemaCatalog, (StatusCode, Json<ErrorResponse>)> {
    SchemaRepository::new(db_pool.clone())
        .catalog()
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("repository error: {error}"),
                    line: None,
                    col: None,
                }),
            )
        })
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,server=debug"));

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn read_env(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::MissingVar(name))
}

fn parse_bind_addr(value: &str) -> Result<SocketAddr, ConfigError> {
    value.parse().map_err(ConfigError::InvalidBindAddr)
}

fn parse_bool_env(value: &str) -> Result<bool, ConfigError> {
    match value {
        "true" | "TRUE" | "True" | "1" => Ok(true),
        "false" | "FALSE" | "False" | "0" => Ok(false),
        _ => Err(ConfigError::InvalidBool {
            name: "SEED_ON_START",
            value: value.to_owned(),
        }),
    }
}

async fn create_db_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().connect(database_url).await
}

async fn check_db_readiness(db_pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(db_pool).await?;
    info!("database readiness check passed");
    Ok(())
}

fn resolve_web_dist_dir() -> Result<PathBuf, ConfigError> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| ConfigError::MissingWebDist(PathBuf::from("web/dist")))?;
    let web_dist_dir = repo_root.join("web").join("dist");

    if web_dist_dir.is_dir() {
        Ok(web_dist_dir)
    } else {
        Err(ConfigError::MissingWebDist(web_dist_dir))
    }
}

fn static_assets_service(web_dist_dir: &Path) -> ServeDir<ServeFile> {
    let index_file = web_dist_dir.join("index.html");

    ServeDir::new(web_dist_dir).fallback(ServeFile::new(index_file))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{execute_cypher_request, execute_schema_request, CypherRequest};
    use axum::http::StatusCode;
    use serde_json::json;
    use serde_json::Value as JsonValue;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use uuid::Uuid;

    use server::domain::{Edge, Node, Properties, SchemaCatalog};
    use server::repository::{EdgeRepository, NodeRepository};
    use server::MIGRATOR;

    #[tokio::test]
    async fn cypher_request_returns_parse_error_shape() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };

        let error = execute_cypher_request(
            &pool,
            CypherRequest {
                query: r#"MATCH (n) WHERE n.name = "Alice""#.to_owned(),
                params: None,
            },
        )
        .await
        .expect_err("request should fail");

        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(error.1 .0.error.contains("expected"));
        assert!(error.1 .0.line.is_some());
        assert!(error.1 .0.col.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn cypher_request_executes_and_returns_query_result() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("cypher");
        seed_graph(&pool, &fixture).await?;

        let result = execute_cypher_request(
            &pool,
            CypherRequest {
                query: format!(
                    r#"
                    MATCH (n:Person {{name: "{name}"}})-[r:KNOWS]->(m:Person)
                    RETURN n, r, m.name
                    LIMIT 10
                "#,
                    name = fixture.alice
                ),
                params: Some(json!({"ignored": true})),
            },
        )
        .await
        .expect("request should succeed");

        assert_eq!(result.columns, vec!["n", "r", "m.name"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.graph.nodes.len(), 1);
        assert_eq!(result.graph.edges.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn cypher_request_returns_execution_error_shape() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };

        let error = execute_cypher_request(
            &pool,
            CypherRequest {
                query: r#"MERGE (n:Person {email: "a", id: 1}) RETURN n"#.to_owned(),
                params: None,
            },
        )
        .await
        .expect_err("request should fail");

        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(error.1 .0.error.contains("MERGE"));
        assert_eq!(error.1 .0.line, None);
        assert_eq!(error.1 .0.col, None);

        Ok(())
    }

    #[tokio::test]
    async fn schema_request_returns_empty_catalog() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };

        let result = execute_schema_request(&pool)
            .await
            .expect("schema request should succeed");

        assert_eq!(
            result,
            SchemaCatalog {
                labels: Vec::new(),
                relationship_types: Vec::new(),
            }
        );

        Ok(())
    }

    #[tokio::test]
    async fn schema_request_returns_catalog_counts() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("schema");
        seed_graph(&pool, &fixture).await?;

        let result = execute_schema_request(&pool)
            .await
            .expect("schema request should succeed");

        assert_eq!(result.labels.len(), 1);
        assert_eq!(result.labels[0].label, "Person");
        assert_eq!(result.labels[0].count, 2);
        assert_eq!(result.relationship_types.len(), 1);
        assert_eq!(result.relationship_types[0].type_, "KNOWS");
        assert_eq!(result.relationship_types[0].count, 1);

        Ok(())
    }

    async fn test_pool() -> Result<Option<PgPool>, sqlx::Error> {
        let database_url = match std::env::var("DATABASE_URL") {
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
                eprintln!("skipping cypher endpoint test setup: {error}");
                return Ok(None);
            }
        };

        if let Err(error) = MIGRATOR.run(&pool).await {
            eprintln!("skipping cypher endpoint migrations: {error}");
            return Ok(None);
        }

        Ok(Some(pool))
    }

    async fn seed_graph(pool: &PgPool, fixture: &Fixture) -> Result<(), sqlx::Error> {
        let node_repository = NodeRepository::new(pool.clone());
        let edge_repository = EdgeRepository::new(pool.clone());

        let alice = sample_node(
            Uuid::from_u128(fixture.base + 1),
            vec!["Person"],
            json!({"name": fixture.alice.clone(), "age": 31}),
        );
        let bob = sample_node(
            Uuid::from_u128(fixture.base + 2),
            vec!["Person"],
            json!({"name": fixture.bob.clone(), "age": 30}),
        );

        node_repository.insert(&alice).await?;
        node_repository.insert(&bob).await?;
        edge_repository
            .insert(&sample_edge(
                Uuid::from_u128(fixture.base + 11),
                alice.id,
                bob.id,
                "KNOWS",
                json!({"since": 2020}),
            ))
            .await?;

        Ok(())
    }

    fn sample_node(id: Uuid, labels: Vec<&str>, properties: JsonValue) -> Node {
        Node {
            id,
            labels: labels.into_iter().map(str::to_owned).collect(),
            properties: as_properties(properties),
        }
    }

    fn sample_edge(
        id: Uuid,
        start_id: Uuid,
        end_id: Uuid,
        edge_type: &str,
        properties: JsonValue,
    ) -> Edge {
        Edge {
            id,
            start_id,
            end_id,
            type_: edge_type.to_owned(),
            properties: as_properties(properties),
        }
    }

    fn as_properties(value: JsonValue) -> Properties {
        match value {
            JsonValue::Object(map) => map,
            _ => panic!("expected JSON object"),
        }
    }

    #[derive(Clone, Debug)]
    struct Fixture {
        base: u128,
        alice: String,
        bob: String,
    }

    fn fixture(prefix: &str) -> Fixture {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(1);

        Fixture {
            base: nanos,
            alice: format!("Alice-{prefix}-{nanos}"),
            bob: format!("Bob-{prefix}-{nanos}"),
        }
    }
}
