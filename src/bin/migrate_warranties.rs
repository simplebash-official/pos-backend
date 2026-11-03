//! Runs the one-time MongoDB move to warranty records (`warranties::migration`) against the
//! configured database. Idempotent.
//!
//!   cargo run --bin migrate_warranties -- --dry-run   # read only: counts and problems
//!   cargo run --bin migrate_warranties                # apply
//!
//! Run before deploying the warranty backend to an environment with existing data: every
//! product needs its `warranty` profile, or it no longer loads.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::warranties::migration::migrate_mongo;

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
        "{} warranty migration on database '{}'",
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
        .expect("warranty migration failed");
    println!(
        "  products given a warranty profile: {} ({} with a warranty)",
        report.products, report.products_with_warranty
    );
    println!(
        "  warranty records from sold serials: {}",
        report.warranties
    );
    for reason in &report.skipped {
        println!("  skipped: {reason}");
    }
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
