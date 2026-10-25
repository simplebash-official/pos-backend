// Runs the one-time MongoDB migration to supplier receipts
// (`purchases::service::receipt_migration`) against the configured database
// (`MONGODB_URI` + `MONGODB_DB_NAME`). Idempotent; safe to re-run.
//
//   cargo run --bin migrate_purchase_receipts -- --dry-run   # read only: report what would change
//   cargo run --bin migrate_purchase_receipts                # apply
//
// MUST run before deploying the receipt-aware backend to an environment with existing data.

use simplebash_pos_backend::{
    clients,
    core::config::Config,
    modules::{
        inventory::service::stock_batch_migration::Mode,
        purchases::service::receipt_migration::migrate_mongo,
    },
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    let config = Config::from_env().expect("invalid configuration");
    println!(
        "{} purchase receipt migration on database '{}'",
        if dry_run {
            "DRY RUN (nothing is written):"
        } else {
            "APPLYING"
        },
        config.mongodb_db_name
    );
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    let mode = if dry_run { Mode::DryRun } else { Mode::Apply };
    let summary = migrate_mongo(&db, mode)
        .await
        .expect("purchase receipt migration failed");

    if summary.purchases == 0 && summary.line_totals == 0 {
        println!("Nothing to migrate: every purchase has a receipt and a line total.");
        return;
    }
    let verb = if dry_run { "would" } else { "did" };
    println!(
        "  purchases given a receipt ({verb}): {}",
        summary.purchases
    );
    println!("  receipts created          ({verb}): {}", summary.receipts);
    println!(
        "  lines given a line total  ({verb}): {}",
        summary.line_totals
    );
}
