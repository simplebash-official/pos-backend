use chrono::Utc;
use jana2u_pos_backend::{
    clients,
    core::{config::Config, constants::prefixes, id::generate_id},
};
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

    let secret_key: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();
    let model_key = generate_id(prefixes::API_KEY);

    let now_iso = Utc::now().to_rfc3339();
    db.collection::<mongodb::bson::Document>("api_keys")
        .insert_one(doc! {
            "key": &model_key,
            "secret": &secret_key,
            "created_at": &now_iso,
            "updated_at": &now_iso,
        })
        .await
        .expect("failed to insert api key");

    println!("seeded api key: key={model_key}, secret={secret_key}");
}
