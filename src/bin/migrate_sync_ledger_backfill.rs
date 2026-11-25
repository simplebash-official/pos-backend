//! Publishes the money ledgers and shop rules already stored in the configured MongoDB database to
//! every shop's sync change feed (`sync::migration`). Idempotent.
//!
//!   cargo run --bin migrate_sync_ledger_backfill -- --dry-run   # read only: counts
//!   cargo run --bin migrate_sync_ledger_backfill                # apply
//!
//! Run once, after deploying the backend that can send these records to devices: rows written
//! before it never entered a change feed, so desktops that are already linked would not get them.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::sync::migration::migrate_mongo;

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
        "{} sync ledger backfill on database '{}'",
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

    let report = migrate_mongo(&db, mode)
        .await
        .expect("sync ledger backfill failed");
    for (shop, documents) in &report.shops {
        println!("  {shop}: {documents} records");
    }
    println!("  shops: {}", report.shops.len());
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
