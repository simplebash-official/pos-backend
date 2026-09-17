use myrologic_pos_backend::{clients, core::config::Config, seeds};

/// Bootstraps the entire POS system database:
/// 1. Admin account (`admin@pos.com` / `admin@1234`)
/// 2. Reference Categories & Subcategories (providers)
/// 3. Suppliers
/// 4. Customers (20 realistic profiles)
/// 5. Inventory Categories, Subcategories & Products
/// 6. API Key
///
/// Safe to run against SQLite or MongoDB.
/// Run with: `cargo run --bin seed_all`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    println!("============================================================");
    println!("🌱 Starting full POS database seeding...");
    println!("============================================================");

    match seeds::seed_all(&db).await {
        Ok(summary) => {
            println!("\n✅ Master seeding completed successfully!");
            println!("------------------------------------------------------------");
            println!("👤 Admin: {}", summary.admin.message);
            println!(
                "📂 Providers: {} categories, {} subcategories created ({} existing)",
                summary.providers.categories_created,
                summary.providers.subcategories_created,
                summary.providers.categories_existing
            );
            println!(
                "🏢 Suppliers: {} created, {} existing",
                summary.suppliers.suppliers_created, summary.suppliers.suppliers_existing
            );
            println!(
                "👥 Customers: {} created, {} existing",
                summary.customers.customers_created, summary.customers.customers_existing
            );
            println!(
                "📦 Inventory: {} categories, {} subcategories, {} products created ({} products existing)",
                summary.inventory.categories_created,
                summary.inventory.subcategories_created,
                summary.inventory.products_created,
                summary.inventory.products_existing
            );
            println!("🔑 API Key: key={}", summary.api_key.key);
            println!("============================================================");
            println!("🚀 System is ready for use with admin@pos.com / admin@1234");
            println!("============================================================");
        }
        Err(err) => panic!("❌ Failed to seed database: {err}"),
    }
}
