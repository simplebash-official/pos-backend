//! One-time MongoDB migration that writes the fields old documents never had, so every
//! stored document loads without `#[serde(default)]`. Idempotent: a complete document is
//! never touched.
//!
//!   cargo run --bin migrate_stored_defaults -- --dry-run   # read only: report what would change
//!   cargo run --bin migrate_stored_defaults                # apply
//!
//! MUST run before deploying the backend without `#[serde(default)]` to an environment
//! with existing data, and `check_data_shape` must report nothing afterwards. Exits with
//! status 1 when a document cannot be repaired (a missing field with no safe value, or a
//! value of the wrong type); those documents are left as they are.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::stored_shape::{Mode, scan};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    let mode = if dry_run { Mode::Report } else { Mode::Apply };
    let config = Config::from_env().expect("invalid configuration");
    let (mongo_uri, mongo_name) = config
        .mongo_settings()
        .expect("this tool needs DATABASE_TYPE=mongodb");
    println!(
        "{} stored-document migration on database '{mongo_name}'",
        if dry_run {
            "DRY RUN (nothing is written):"
        } else {
            "APPLYING"
        }
    );
    let db = clients::mongo::connect(mongo_uri, mongo_name)
        .await
        .expect("failed to connect to MongoDB");
    let report = scan(&db, mode).await.expect("the migration failed");
    print!(
        "{}",
        report.render(if dry_run { "would get" } else { "got" })
    );
    if dry_run {
        println!("\nNothing was changed. Run without --dry-run to apply.");
    }
    if report.collections.iter().any(|c| c.unrepairable > 0) {
        std::process::exit(1);
    }
}
