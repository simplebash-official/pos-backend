//! Seeds the suppliers collection/table with sample suppliers.
//! Upserts idempotently, safe to run against SQLite or MongoDB.
//!
//! Run with: `cargo run --bin seed_suppliers`

use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::{clients, seeds};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    match seeds::suppliers::seed_suppliers(&db).await {
        Ok(res) => {
            println!(
                "seeded suppliers: {} created, {} existing",
                res.suppliers_created, res.suppliers_existing
            );
        }
        Err(err) => panic!("failed to seed suppliers: {err}"),
    }
}
