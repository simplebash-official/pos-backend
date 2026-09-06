use jana2u_pos_backend::{clients, core::config::Config, seeds};

/// Seeds the inventory categories, subcategories, and products into the database.
/// Upserts idempotently, safe to run against SQLite or MongoDB.
///
/// Run with: `cargo run --bin seed_inventory`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    println!("🌱 Starting inventory seeding via codebase services...");

    match seeds::inventory::seed_inventory(&db).await {
        Ok(res) => {
            println!("\n🎉 Seeding complete!");
            println!("   New categories created: {}", res.categories_created);
            println!(
                "   New subcategories created: {}",
                res.subcategories_created
            );
            println!("   New products created: {}", res.products_created);
            println!("   Products existing: {}", res.products_existing);
        }
        Err(err) => panic!("failed to seed inventory: {err}"),
    }
}
