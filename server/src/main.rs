use std::env;
use std::error::Error;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};

use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{Path, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use server::domain::{Edge, Node, SchemaCatalog};
use server::executor::QueryResult;
use server::parser::parse_ast;
use server::planner::plan_query;
use server::repository::{EdgeRepository, NodeRepository, SchemaRepository};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
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

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
    line: Option<usize>,
    col: Option<usize>,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    error: String,
    line: Option<usize>,
    col: Option<usize>,
}

impl AppError {
    fn new(status: StatusCode, error: impl Into<String>) -> Self {
        Self {
            status,
            error: error.into(),
            line: None,
            col: None,
        }
    }

    fn with_position(
        status: StatusCode,
        error: impl Into<String>,
        line: Option<usize>,
        col: Option<usize>,
    ) -> Self {
        Self {
            status,
            error: error.into(),
            line,
            col,
        }
    }

    fn bad_request(error: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, error)
    }

    fn not_found(error: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, error)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.error,
                line: self.line,
                col: self.col,
            }),
        )
            .into_response()
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        Self::bad_request(format!("invalid JSON request body: {rejection}"))
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        Self::bad_request(format!("invalid path parameter: {rejection}"))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct NodeNeighborsResponse {
    node: Node,
    edges: Vec<Edge>,
    nodes: Vec<Node>,
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
        .route("/node/:id", get(node_details))
        .route("/cypher", post(cypher))
        .fallback_service(static_assets_service(&web_dist_dir))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
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
) -> Result<Json<SchemaCatalog>, AppError> {
    execute_schema_request(&state.db_pool).await.map(Json)
}

async fn node_details(
    State(state): State<AppState>,
    node_id: Result<Path<uuid::Uuid>, PathRejection>,
) -> Result<Json<NodeNeighborsResponse>, AppError> {
    let Path(node_id) = node_id.map_err(AppError::from)?;
    execute_node_request(&state.db_pool, node_id).await.map(Json)
}

async fn cypher(
    State(state): State<AppState>,
    request: Result<Json<CypherRequest>, JsonRejection>,
) -> Result<Json<QueryResult>, AppError> {
    let Json(request) = request.map_err(AppError::from)?;
    execute_cypher_request(&state.db_pool, request)
        .await
        .map(Json)
}

async fn execute_cypher_request(
    db_pool: &PgPool,
    request: CypherRequest,
) -> Result<QueryResult, AppError> {
    let _ = &request.params;

    let ast = parse_ast(&request.query).map_err(|error| {
        AppError::with_position(
            StatusCode::BAD_REQUEST,
            error.message,
            Some(error.line),
            Some(error.column),
        )
    })?;

    let plan = plan_query(&ast).map_err(|error| AppError::bad_request(error.message))?;

    server::executor::execute_plan(db_pool.clone(), &plan)
        .await
        .map_err(|error| AppError::bad_request(error.message))
}

async fn execute_schema_request(db_pool: &PgPool) -> Result<SchemaCatalog, AppError> {
    SchemaRepository::new(db_pool.clone())
        .catalog()
        .await
        .map_err(|error| AppError::bad_request(format!("repository error: {error}")))
}

async fn execute_node_request(
    db_pool: &PgPool,
    node_id: uuid::Uuid,
) -> Result<NodeNeighborsResponse, AppError> {
    let node_repository = NodeRepository::new(db_pool.clone());
    let edge_repository = EdgeRepository::new(db_pool.clone());

    let node = node_repository
        .get_by_id(node_id)
        .await
        .map_err(|error| AppError::bad_request(format!("repository error: {error}")))?
        .ok_or_else(|| AppError::not_found(format!("node not found: {node_id}")))?;

    let neighbors = edge_repository
        .expand_neighbors(node_id, None)
        .await
        .map_err(|error| AppError::bad_request(format!("repository error: {error}")))?;

    Ok(NodeNeighborsResponse {
        node,
        edges: neighbors.edges,
        nodes: neighbors.nodes,
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
    let repo_root = FsPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| ConfigError::MissingWebDist(PathBuf::from("web/dist")))?;
    let web_dist_dir = repo_root.join("web").join("dist");

    if web_dist_dir.is_dir() {
        Ok(web_dist_dir)
    } else {
        Err(ConfigError::MissingWebDist(web_dist_dir))
    }
}

fn static_assets_service(web_dist_dir: &FsPath) -> ServeDir<ServeFile> {
    let index_file = web_dist_dir.join("index.html");

    ServeDir::new(web_dist_dir).fallback(ServeFile::new(index_file))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        app_router, AppConfig, ErrorResponse, NodeNeighborsResponse,
    };
    use axum::body::{to_bytes, Body};
    use axum::http::StatusCode;
    use axum::http::{Method, Request};
    use serde_json::json;
    use serde_json::Value as JsonValue;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use tower::ServiceExt;
    use uuid::Uuid;

    use server::domain::{Edge, Node, Properties, SchemaCatalog};
    use server::executor::QueryResult;
    use server::repository::{EdgeRepository, NodeRepository};
    use server::MIGRATOR;

    #[tokio::test]
    async fn cypher_route_executes_read_query() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("cypher-read");
        let _ = seed_graph(&pool, &fixture).await?;
        let response = request(
            test_app(pool),
            Method::POST,
            "/cypher",
            Some(json!({
                "query": format!(
                    r#"
                    MATCH (n:Person {{name: "{name}"}})-[r:KNOWS]->(m:Person)
                    RETURN n, r, m.name
                    LIMIT 10
                "#,
                    name = fixture.alice
                )
            })),
        )
        .await;

        let status = response.status();
        let payload: QueryResult = response_json(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload.columns, vec!["n", "r", "m.name"]);
        assert_eq!(payload.rows.len(), 1);
        assert_eq!(payload.graph.nodes.len(), 1);
        assert_eq!(payload.graph.edges.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn cypher_route_executes_write_query() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("cypher-write");
        let response = request(
            test_app(pool.clone()),
            Method::POST,
            "/cypher",
            Some(json!({
                "query": format!(
                    r#"
                    CREATE (n:Person {{name: "{name}", age: 28}})
                    RETURN n
                "#,
                    name = fixture.alice
                )
            })),
        )
        .await
        ;

        let status = response.status();
        let payload: QueryResult = response_json(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload.columns, vec!["n"]);
        assert_eq!(payload.rows.len(), 1);
        assert_eq!(payload.graph.nodes.len(), 1);
        assert_eq!(payload.graph.edges.len(), 0);

        let persisted = NodeRepository::new(pool)
            .scan_by_label("Person", 10)
            .await?
            .into_iter()
            .find(|node| {
                node.properties
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .is_some_and(|name| name == fixture.alice)
            });
        assert!(persisted.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn cypher_route_returns_parse_error_shape() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let response = request(
            test_app(pool),
            Method::POST,
            "/cypher",
            Some(json!({
                "query": r#"MATCH (n) WHERE n.name = "Alice""#
            })),
        )
        .await;

        let status = response.status();
        let payload: ErrorResponse = response_json(response).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(payload.error.contains("expected"));
        assert!(payload.line.is_some());
        assert!(payload.col.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn schema_route_returns_catalog_counts() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("schema-route");
        let _ = seed_graph(&pool, &fixture).await?;
        let response = request(test_app(pool), Method::GET, "/schema", None).await;

        let status = response.status();
        let payload: SchemaCatalog = response_json(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload.labels.len(), 1);
        assert_eq!(payload.labels[0].label, "Person");
        assert_eq!(payload.labels[0].count, 2);
        assert_eq!(payload.relationship_types.len(), 1);
        assert_eq!(payload.relationship_types[0].type_, "KNOWS");
        assert_eq!(payload.relationship_types[0].count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn node_route_returns_center_node_and_neighbors() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("node-route");
        let (alice, bob, edge_id) = seed_graph(&pool, &fixture).await?;
        let response = request(
            test_app(pool),
            Method::GET,
            &format!("/node/{}", alice.id),
            None,
        )
        .await;

        let status = response.status();
        let payload: NodeNeighborsResponse = response_json(response).await;

        assert_eq!(
            status,
            StatusCode::OK
        );
        assert_eq!(
            payload,
            NodeNeighborsResponse {
                node: alice,
                edges: vec![sample_edge(
                    edge_id,
                    Uuid::from_u128(fixture.base + 1),
                    Uuid::from_u128(fixture.base + 2),
                    "KNOWS",
                    json!({"since": 2020}),
                )],
                nodes: vec![bob],
            }
        );

        Ok(())
    }

    #[tokio::test]
    async fn node_route_returns_not_found_error() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let response = request(
            test_app(pool),
            Method::GET,
            &format!("/node/{}", Uuid::from_u128(999_999)),
            None,
        )
        .await;
        let status = response.status();
        let payload: ErrorResponse = response_json(response).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(payload.error.contains("node not found"));
        assert_eq!(payload.line, None);
        assert_eq!(payload.col, None);

        Ok(())
    }

    #[tokio::test]
    async fn cypher_route_returns_execution_error_shape() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let response = request(
            test_app(pool),
            Method::POST,
            "/cypher",
            Some(json!({
                "query": r#"MERGE (n:Person {email: "a", id: 1}) RETURN n"#
            })),
        )
        .await;

        let status = response.status();
        let payload: ErrorResponse = response_json(response).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(payload.error.contains("MERGE"));
        assert_eq!(payload.line, None);
        assert_eq!(payload.col, None);

        Ok(())
    }

    #[tokio::test]
    async fn app_router_returns_json_error_for_invalid_request_body() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let app = test_app(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/cypher")
                    .header("content-type", "application/json")
                    .body(Body::from("{\"query\":"))
                    .expect("request should build"),
            )
            .await
            .expect("request should complete");

        let status = response.status();
        let payload: ErrorResponse = response_json(response).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(payload.error.contains("invalid JSON request body"));
        assert_eq!(payload.line, None);
        assert_eq!(payload.col, None);

        Ok(())
    }

    #[tokio::test]
    async fn app_router_applies_cors_headers() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let app = test_app(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/cypher")
                    .header("origin", "http://localhost:3000")
                    .header("access-control-request-method", "POST")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|value| value.to_str().ok()),
            Some("*")
        );
        assert!(
            response
                .headers()
                .contains_key("access-control-allow-methods")
        );

        Ok(())
    }

    async fn response_json<T>(response: axum::response::Response) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        serde_json::from_slice(&body).expect("body should contain valid JSON")
    }

    async fn request(
        app: axum::Router,
        method: Method,
        uri: &str,
        body: Option<JsonValue>,
    ) -> axum::response::Response {
        let mut builder = Request::builder().method(method).uri(uri);

        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }

        app.oneshot(
            builder
                .body(match body {
                    Some(value) => Body::from(
                        serde_json::to_vec(&value).expect("request JSON should serialize"),
                    ),
                    None => Body::empty(),
                })
                .expect("request should build"),
        )
        .await
        .expect("request should complete")
    }

    fn test_app(pool: PgPool) -> axum::Router {
        app_router(test_config(), pool)
    }

    fn test_config() -> AppConfig {
        AppConfig {
            bind_addr: "127.0.0.1:8080"
                .parse()
                .expect("test bind address should parse"),
            database_url: "postgres://example.invalid/test".to_owned(),
            seed_on_start: false,
            web_dist_dir: PathBuf::from("/workspace/web/dist"),
        }
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

    async fn seed_graph(pool: &PgPool, fixture: &Fixture) -> Result<(Node, Node, Uuid), sqlx::Error> {
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
        let edge_id = Uuid::from_u128(fixture.base + 11);
        edge_repository
            .insert(&sample_edge(
                edge_id,
                alice.id,
                bob.id,
                "KNOWS",
                json!({"since": 2020}),
            ))
            .await?;

        Ok((alice, bob, edge_id))
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
