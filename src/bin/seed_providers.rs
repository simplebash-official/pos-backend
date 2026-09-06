use jana2u_pos_backend::{clients, core::config::Config, seeds};

/// Seeds the categories and subcategories reference data into the database.
/// Upserts idempotently, safe to run against SQLite or MongoDB.
///
/// Run with: `cargo run --bin seed_providers`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    match seeds::providers::seed_providers(&db).await {
        Ok(res) => {
            println!(
                "seeded providers: {} categories created, {} subcategories created, {} categories existing",
                res.categories_created, res.subcategories_created, res.categories_existing
            );
        }
        Err(err) => panic!("failed to seed providers: {err}"),
    }
}
