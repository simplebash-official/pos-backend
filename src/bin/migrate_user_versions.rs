//! Gives a version to every account in the configured MongoDB database that was stored without
//! one, so its shop can be downloaded by a desktop and linked desktops receive it
//! (`sync::migration`). Idempotent.
//!
//!   cargo run --bin migrate_user_versions -- --dry-run   # read only: counts
//!   cargo run --bin migrate_user_versions                # apply
//!
//! Run once, after deploying the backend that stores new accounts with a version.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::sync::migration::migrate_user_versions;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    let mode = if dry_run { Mode::DryRun } else { Mode::Apply };
    let config = Config::from_env().expect("invalid configuration");
    let (_, mongo_name) = config
        .mongo_settings()
        .expect("this tool needs DATABASE_TYPE=mongodb");
    println!(
        "{} user version backfill on database '{}'",
        if dry_run {
            "DRY RUN (nothing is written):"
        } else {
            "APPLYING"
        },
        mongo_name
    );
    let db = clients::connect_from_config(&config)
        .await
        .expect("failed to connect to MongoDB");

    let report = migrate_user_versions(&db, mode)
        .await
        .expect("user version backfill failed");
    for (shop, accounts) in &report.shops {
        println!("  {shop}: {accounts} accounts");
    }
    println!("  shops: {}", report.shops.len());
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
