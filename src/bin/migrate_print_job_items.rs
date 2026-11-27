//! Runs the one-time MongoDB move of print jobs to items (`print_jobs::migration`) against the
//! configured database, for every shop in it. Idempotent.
//!
//!   cargo run --bin migrate_print_job_items -- --dry-run   # read only: counts
//!   cargo run --bin migrate_print_job_items                # apply
//!
//! Run before deploying the print-job-items backend to an environment with existing data: a job
//! without `items` no longer loads, and the previous backend cannot load a migrated one.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::print_jobs::migration::migrate_items_mongo;

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
        "{} print job items migration on database '{}'",
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

    let report = migrate_items_mongo(&db, mode)
        .await
        .expect("print job items migration failed");
    println!(
        "  print jobs moved to items: {} ({} with a price or material cost)",
        report.print_jobs, report.with_items
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
