use jana2u_pos_backend::{clients, core::config::Config};

/// Placeholder seeding routine for reference/lookup data (e.g. repair
/// service providers). Run with: `cargo run --bin seed_providers`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let _db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    println!("seed_providers: no seed data defined yet");
}
