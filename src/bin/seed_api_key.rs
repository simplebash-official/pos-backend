use chrono::Utc;
use jana2u_pos_backend::{clients, core::config::Config};
use mongodb::bson::doc;
use rand::{RngExt, distr::Alphanumeric};

/// Generates a random API key and inserts it into the `api_keys` collection.
/// Run with: `cargo run --bin seed_api_key`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    let key: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();

    db.collection::<mongodb::bson::Document>("api_keys")
        .insert_one(doc! {
            "key": &key,
            "created_at": Utc::now().to_rfc3339(),
        })
        .await
        .expect("failed to insert api key");

    println!("seeded api key: {key}");
}
