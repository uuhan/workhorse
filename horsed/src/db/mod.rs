use anyhow::Context;
use once_cell::sync::Lazy;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use stable::prelude::handle;
use std::time::Duration;

pub mod entity;

pub static DB: Lazy<DatabaseConnection> = Lazy::new(|| {
    let url = "sqlite://horsed.db3?mode=rwc";
    let mut opt = ConnectOptions::new(url);
    opt.connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(8))
        .max_lifetime(Duration::from_secs(8))
        .sqlx_logging(true)
        .set_schema_search_path("my_schema"); // Setting default PostgreSQL schema

    handle()
        .block_on(Database::connect(opt))
        .context(format!("DB URL: {}", url))
        .expect("Failed to connect to database")
});

pub fn db() -> DatabaseConnection {
    DB.clone()
}
