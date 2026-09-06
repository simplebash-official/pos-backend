use jana2u_pos_backend::{clients, core::config::Config, seeds};

/// Generates a random API key and inserts it into the `api_keys` table/collection.
/// Safe to run against SQLite or MongoDB.
///
/// Run with: `cargo run --bin seed_api_key`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    match seeds::api_key::seed_api_key(&db).await {
        Ok(res) => println!("seeded api key: key={}, secret={}", res.key, res.secret),
        Err(err) => panic!("failed to seed api key: {err}"),
    }
}
