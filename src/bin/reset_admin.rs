use simplebash_pos_backend::{
    clients, core::config::Config, modules::users::service::reset_admin_credentials,
};

/// Rotates the existing Admin account's email/password to
/// `RESET_ADMIN_EMAIL` (default `admin@pos.com`) / `RESET_ADMIN_PASSWORD`
/// (required — there is deliberately no default, so a reset can never leave
/// the shop on a publicly documented password).
///
/// Unlike `seed_admin`, this requires an Admin to already exist and
/// overwrites its credentials in place — there is no undo, and no HTTP
/// route can do this (an Admin account is never manageable through the
/// API by anyone, including another Admin). Safe to run against SQLite or
/// MongoDB.
///
/// Run with: `cargo run --bin reset_admin`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let email = std::env::var("RESET_ADMIN_EMAIL").unwrap_or_else(|_| "admin@pos.com".to_string());
    let password = match std::env::var("RESET_ADMIN_PASSWORD") {
        Ok(p) if p.trim().len() >= 8 => p,
        _ => {
            eprintln!(
                "RESET_ADMIN_PASSWORD must be set to the new admin password (at least 8 characters)"
            );
            std::process::exit(2);
        }
    };

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    match reset_admin_credentials(&db, &email, &password).await {
        Ok(user) => println!("Admin credentials updated: {}", user.email),
        Err(err) => panic!("failed to reset admin credentials: {err}"),
    }
}
