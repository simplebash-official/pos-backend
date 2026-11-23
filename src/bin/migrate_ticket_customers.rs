//! Runs the one-time MongoDB move that links every repair job and print job saved without a
//! customer to a customer record (`customers::migration`) on the configured database.
//! Idempotent.
//!
//!   cargo run --bin migrate_ticket_customers -- --dry-run   # read only: counts
//!   cargo run --bin migrate_ticket_customers                # apply
//!
//! Run before deploying the backend that requires a customer on every ticket to an
//! environment with existing data: a ticket without `customer_key` no longer loads.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::customers::migration::migrate_mongo;
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
        "{} ticket customers migration on database '{}'",
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
        .expect("ticket customers migration failed");
    println!(
        "  tickets linked to a customer: {} ({} new customers created)",
        report.tickets, report.customers_created
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
