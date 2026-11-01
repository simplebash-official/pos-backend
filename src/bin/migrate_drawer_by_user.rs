//! Runs the one-time MongoDB migration of drawer balances from one per machine to one per user
//! (`sessions::migration`) against the configured database. Idempotent.
//!
//!   cargo run --bin migrate_drawer_by_user -- --dry-run   # read only: show every planned balance
//!   cargo run --bin migrate_drawer_by_user                # apply
//!
//! Run right after deploying the per-user drawer backend to an environment with existing data.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::inventory::service::stock_batch_migration::Mode;
use simplebash_pos_backend::modules::sessions::migration::migrate_mongo;

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
        "{} drawer-per-user migration on database '{}'",
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
        .expect("drawer migration failed");
    println!("  per-machine rows found: {}", report.machine_rows);
    for planned in &report.planned {
        println!(
            "  shop {} user {}: {} cents (from machine {})",
            planned.tenant_id.as_deref().unwrap_or("(single shop)"),
            planned.user_key,
            planned.balance_cents,
            planned.from_terminal_id
        );
    }
    for terminal in &report.ownerless {
        println!("  dropped, machine never had a shift: {terminal}");
    }
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
