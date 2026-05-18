use std::env;
use std::error::Error;
use std::path::PathBuf;

use server::seed::{load_seed_data, SeedReport, SeedStatus};
use server::MIGRATOR;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let data_dir = env::var("DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/data"));
    std::fs::create_dir_all(&data_dir)?;
    let db_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(data_dir.join("graph.db"))
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal),
        )
        .await?;

    MIGRATOR.run(&db_pool).await?;

    let report = load_seed_data(&db_pool, true).await?;
    log_seed_report(&report);

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,server=debug"));

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn log_seed_report(report: &SeedReport) {
    match report.status {
        SeedStatus::SkippedDisabled => {
            info!("seed binary skipped because SEED_ON_START is false");
        }
        SeedStatus::SkippedSentinel => {
            info!("seed binary skipped because seed sentinel already exists");
        }
        SeedStatus::Loaded => {
            info!(
                datasets = report.datasets,
                nodes_created = report.nodes_created,
                nodes_matched = report.nodes_matched,
                edges_created = report.edges_created,
                edges_matched = report.edges_matched,
                "seed binary completed",
            );
        }
    }
}
