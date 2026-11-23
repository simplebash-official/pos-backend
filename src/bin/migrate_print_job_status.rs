//! Runs the one-time MongoDB move of print jobs saved as `in_progress` (a status that was never
//! meant to exist) to `in_repair`, shown as "In production", on the configured database, for
//! every shop in it. Idempotent.
//!
//!   cargo run --bin migrate_print_job_status -- --dry-run   # read only: counts
//!   cargo run --bin migrate_print_job_status                # apply
//!
//! Run when deploying the backend that stops accepting `in_progress`: the print job screens
//! cannot show a job that still holds it.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::print_jobs::migration::{
    CURRENT_STATUS, LEGACY_STATUS, migrate_mongo,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    let mode = if dry_run { Mode::DryRun } else { Mode::Apply };
    let config = Config::from_env().expect("invalid configuration");
    let (mongo_uri, mongo_name) = config
        .mongo_settings()
        .expect("this tool needs DATABASE_TYPE=mongodb");
    println!(
        "{} print job status migration on database '{}'",
        if dry_run {
            "DRY RUN (nothing is written):"
        } else {
            "APPLYING"
        },
        mongo_name
    );
    let db = clients::mongo::connect(mongo_uri, mongo_name)
        .await
        .expect("failed to connect to MongoDB");

    let report = migrate_mongo(&db, mode)
        .await
        .expect("print job status migration failed");
    println!(
        "  print jobs moved from {LEGACY_STATUS} to {CURRENT_STATUS}: {}",
        report.print_jobs
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
