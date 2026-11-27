//! Runs the one-time MongoDB copy of each invoice's customer onto the credit notes saved without
//! one (`billing::migration`) on the configured database. Idempotent.
//!
//!   cargo run --bin migrate_credit_note_customers -- --dry-run   # read only: counts
//!   cargo run --bin migrate_credit_note_customers                # apply
//!
//! Run after deploying the backend that copies the customer onto new credit notes, and after
//! `cloud_store::MIN_DESKTOP_APP_VERSION` holds back desktops without this change: an older one
//! refuses a later void or resolution of a note filled here.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::billing::migration::migrate_credit_note_customers_mongo;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;

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
        "{} credit note customers migration on database '{}'",
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

    let report = migrate_credit_note_customers_mongo(&db, mode)
        .await
        .expect("credit note customers migration failed");
    println!("  credit notes given a customer: {}", report.credit_notes);
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
