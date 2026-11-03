//! Runs the one-time MongoDB move of repair jobs to items (`repairs::migration`) against the
//! configured database. Idempotent.
//!
//!   cargo run --bin migrate_repair_items -- --dry-run   # read only: counts
//!   cargo run --bin migrate_repair_items                # apply
//!
//! Run before deploying the repair-items backend to an environment with existing data: a job
//! without `items` no longer loads.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::repairs::migration::migrate_mongo;

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
        "{} repair items migration on database '{}'",
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
        .expect("repair items migration failed");
    println!(
        "  repair jobs moved to items: {} ({} with a price or material cost)",
        report.repairs, report.with_items
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
