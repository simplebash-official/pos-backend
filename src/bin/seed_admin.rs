//! Bootstraps the first Admin account with username `admin` and the password from `SEED_ADMIN_PASSWORD`.
//!
//! Create-if-missing: re-running this never resets an existing admin's password or creates
//! duplicate admins. Safe to run against SQLite or MongoDB.
//!
//! Run with: `cargo run --bin seed_admin`

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

    match seeds::admin::seed_admin(&db, &config.seed_admin).await {
        Ok(res) => println!("{}", res.message),
        Err(err) => panic!("failed to seed admin account: {err}"),
    }
}
