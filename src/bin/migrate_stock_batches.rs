// Runs the one-time MongoDB migration to stock batches
// (`inventory::service::stock_batch_migration`) against the configured
// database (`MONGODB_URI` + `MONGODB_DB_NAME`). Idempotent; safe to re-run.
//
//   cargo run --bin migrate_stock_batches -- --dry-run   # read only: report what would change
//   cargo run --bin migrate_stock_batches                # apply
//
// MUST run before deploying the batch-aware backend to an environment with
// existing data. Run the dry run first and read its warnings.

use simplebash_pos_backend::{
    clients,
    core::config::Config,
    modules::inventory::service::stock_batch_migration::{Mode, Summary, migrate_mongo},
};

fn print_summary(label: &str, s: &Summary, dry_run: bool) {
    let verb = if dry_run { "would" } else { "did" };
    println!("{label}");
    println!("  products moved to batches ({verb}): {}", s.products);
    println!("  opening batches created            : {}", s.batches);
    println!("  stock movements pointed at a batch : {}", s.movements);
    println!("  serial numbers pointed at a batch  : {}", s.serials);
    println!("  purchases given a selling price    : {}", s.purchases);
    println!("  invoices with lines allocated      : {}", s.invoices);
    println!("  credit notes with lines allocated  : {}", s.credit_notes);
    if s.zero_price_products > 0 {
        println!(
            "  WARNING: {} product(s) sell at Rs. 0 — their opening batch keeps that price",
            s.zero_price_products
        );
    }
    if s.skipped_products > 0 {
        println!(
            "  WARNING: {} product(s) have no key and will be skipped",
            s.skipped_products
        );
    }
    if s.lines_without_cost > 0 {
        println!(
            "  WARNING: {} old sale/return line(s) have no cost and no migratable product — left without a batch",
            s.lines_without_cost
        );
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    let config = Config::from_env().expect("invalid configuration");
    println!(
        "{} stock batch migration on database '{}'",
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
    let report = migrate_mongo(&db, mode)
        .await
        .expect("stock batch migration failed");

    if report.shops.is_empty() {
        println!("Nothing to migrate: every product already uses stock batches.");
        return;
    }
    for (shop, summary) in &report.shops {
        print_summary(&format!("\nShop {shop}"), summary, dry_run);
    }
    print_summary(
        &format!("\nTOTAL across {} shop(s)", report.shops.len()),
        &report.total(),
        dry_run,
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
}
