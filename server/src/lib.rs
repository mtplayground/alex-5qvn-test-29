pub mod domain;
pub mod parser;
pub mod repository;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");
