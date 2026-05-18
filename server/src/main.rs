use std::env;
use std::error::Error;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Serialize;
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
