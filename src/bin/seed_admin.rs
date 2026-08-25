use std::env;

use jana2u_pos_backend::{
    clients,
    core::{config::Config, error::AppError},
    domain::users::{CreateUserRequest, Role},
    modules::users::service::create_user,
};

/// Bootstraps the first Admin account. Registration is admin-provisioned
/// only (no public `POST /register`), so without this script there would be
/// no way to create the very first user able to call `POST /users`.
///
/// Create-if-missing, unlike `seed_suppliers.rs`'s always-upsert — re-running
/// this must never reset a real deployment's admin password back to a seed
/// value. Requires `SEED_ADMIN_EMAIL`/`SEED_ADMIN_PASSWORD`/`SEED_ADMIN_NAME`
/// to be set (in `.env`, see `.env.example`) — fails fast rather than
/// silently falling back to a hardcoded default password, same as
/// `Config::from_env()` fails fast on a missing `JWT_SECRET`.
///
/// Run with: `cargo run --bin seed_admin`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    let email =
        env::var("SEED_ADMIN_EMAIL").expect("SEED_ADMIN_EMAIL must be set (see .env.example)");
    let name = env::var("SEED_ADMIN_NAME").expect("SEED_ADMIN_NAME must be set (see .env.example)");
    let password = env::var("SEED_ADMIN_PASSWORD")
        .expect("SEED_ADMIN_PASSWORD must be set (see .env.example)");

    let result = create_user(
        &db,
        CreateUserRequest {
            name,
            email: email.clone(),
            password,
            role: Role::Admin,
            employee_key: None,
        },
    )
    .await;

    match result {
        Ok(_) => println!("seeded admin account: {email}"),
        Err(AppError::Custom { code, .. })
            if code == jana2u_pos_backend::core::constants::codes::EMAIL_ALREADY_EXISTS =>
        {
            println!("admin account already exists: {email} — no changes made");
        }
        // This deployment supports only one Admin (see
        // `users::service::create_user`) — re-running with a *different*
        // email than the existing admin's hits this instead of the
        // email-uniqueness case above.
        Err(AppError::Custom { code, .. })
            if code == jana2u_pos_backend::core::constants::codes::ADMIN_ALREADY_EXISTS =>
        {
            println!(
                "an admin account already exists under a different email — no changes made \
                 (this deployment supports only one Admin)"
            );
        }
        Err(err) => panic!("failed to seed admin account: {err}"),
    }
}
