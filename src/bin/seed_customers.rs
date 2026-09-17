use myrologic_pos_backend::{clients, core::config::Config, seeds};

/// Seeds the customers collection/table with 20 realistic sample customers.
/// Upserts idempotently, safe to run against SQLite or MongoDB.
///
/// Run with: `cargo run --bin seed_customers`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    match seeds::customers::seed_customers(&db).await {
        Ok(res) => {
            println!(
                "seeded customers: {} created, {} existing",
                res.customers_created, res.customers_existing
            );
        }
        Err(err) => panic!("failed to seed customers: {err}"),
    }
}
