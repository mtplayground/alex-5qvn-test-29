use std::env;
use std::error::Error;
use std::io;

use server::seed::{load_seed_data, SeedReport, SeedStatus};
use server::MIGRATOR;
use sqlx::postgres::PgPoolOptions;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let database_url = env::var("DATABASE_URL").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "missing required environment variable `DATABASE_URL`",
        )
    })?;
    let db_pool = PgPoolOptions::new().connect(&database_url).await?;

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
