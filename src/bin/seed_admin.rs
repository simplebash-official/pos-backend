use myrologic_pos_backend::{clients, core::config::Config, seeds};

/// Bootstraps the first Admin account with default credentials `admin@pos.com` / `admin@1234`.
///
/// Create-if-missing: re-running this never resets an existing admin's password or creates
/// duplicate admins. Safe to run against SQLite or MongoDB.
///
/// Run with: `cargo run --bin seed_admin`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    match seeds::admin::seed_admin(&db, None, None, None).await {
        Ok(res) => println!("{}", res.message),
        Err(err) => panic!("failed to seed admin account: {err}"),
    }
}
