//! Read-only check that every stored MongoDB document loads as its typed struct.
//!
//!   cargo run --bin check_data_shape
//!
//! Lists, per collection, how many documents lack which field. Run it before deploying a
//! backend without `#[serde(default)]` to an environment with existing data; after
//! `migrate_stored_defaults` it must report nothing. Exits with status 1 when any document
//! is incomplete or cannot be repaired. It never writes and never prints field values.

use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::modules::stored_shape::{Mode, scan};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("invalid configuration");
    let (mongo_uri, mongo_name) = config
        .mongo_settings()
        .expect("this tool needs DATABASE_TYPE=mongodb");
    println!("Checking the shape of stored documents in database '{mongo_name}'");
    let db = clients::mongo::connect(mongo_uri, mongo_name)
        .await
        .expect("failed to connect to MongoDB");
    let report = scan(&db, Mode::Report).await.expect("the scan failed");
    print!("{}", report.render("lack"));
    if !report.is_clean() {
        std::process::exit(1);
    }
}
