// Runs the one-time MongoDB migration of cash movements to typed movements with categories
// (`cash_categories::migration`) against the configured database. Idempotent.
//
//   cargo run --bin migrate_cash_movements -- --dry-run   # read only: report what would change
//   cargo run --bin migrate_cash_movements                # apply
//
// MUST run before deploying the typed-movement backend to an environment with existing data.

use simplebash_pos_backend::{
    clients,
    core::config::{Config, TenantMode},
    modules::cash_categories::migration::migrate_mongo,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    let config = Config::from_env().expect("invalid configuration");
    println!(
        "{} cash movement migration on database '{}'",
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

    let report = migrate_mongo(&db, config.tenant_mode == TenantMode::Multi, dry_run)
        .await
        .expect("cash movement migration failed");
    let verb = if dry_run { "would be" } else { "were" };
    println!(
        "  shops checked for the built-in categories: {}",
        report.shops_checked
    );
    println!(
        "  old movements that {verb} converted: {}",
        report.movements
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
