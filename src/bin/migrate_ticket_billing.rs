//! Runs the one-time MongoDB move that marks every repair job and print job already billed by an
//! invoice with that invoice (`billing::migration`) on the configured database. Idempotent.
//!
//!   cargo run --bin migrate_ticket_billing -- --dry-run   # read only: counts
//!   cargo run --bin migrate_ticket_billing                # apply
//!
//! Run before deploying the backend that records the invoice on every billed ticket to an
//! environment with existing data: without it a ticket billed earlier has no mark, so the guard
//! against billing it twice and the "Billed" marker on the screens do not know about it.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::billing::migration::migrate_mongo;
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
        "{} ticket billing migration on database '{}'",
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
        .expect("ticket billing migration failed");
    println!(
        "  tickets marked with the invoice that billed them: {}",
        report.tickets
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
