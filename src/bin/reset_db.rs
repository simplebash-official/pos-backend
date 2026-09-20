use simplebash_pos_backend::{clients, core::config::Config, seeds};
use std::fs;
use std::path::Path;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    println!("============================================================");
    println!("🗑️  Resetting database for engine: {}", db.engine_name());
    println!("============================================================");

    if let Some(tenant_db) = db.as_mongo() {
        // Maintenance binary: intentionally operates on the whole database.
        let mongo = tenant_db.unscoped();
        println!(
            "Dropping all MongoDB collections in '{}'...",
            config.mongodb_db_name
        );
        let collections = mongo
            .list_collection_names()
            .await
            .expect("failed to list collections");
        for col_name in collections {
            println!("  Dropping collection: {}", col_name);
            mongo
                .collection::<mongodb::bson::Document>(&col_name)
                .drop()
                .await
                .expect("failed to drop collection");
        }
        println!("✅ All MongoDB collections dropped.");

        println!("Ensuring MongoDB indexes...");
        clients::indexes::ensure_indexes(
            mongo,
            config.tenant_mode == simplebash_pos_backend::core::config::TenantMode::Multi,
        )
        .await;
        println!("✅ Indexes recreated.");
    } else if let Some(pool) = db.as_sqlite() {
        println!("Resetting SQLite tables...");
        // In SQLite, clear user transaction/catalog tables if in SQLite mode
        let tables = vec![
            "credit_notes",
            "payments",
            "invoices",
            "stock_movements",
            "product_serials",
            "products",
            "subcategories",
            "categories",
            "customers",
            "suppliers",
            "supplier_products",
            "purchases",
            "repairs",
            "print_jobs",
            "generated_documents",
            "login_sessions",
            "users",
            "api_keys",
            "employees",
            "sequence_counters",
            "sequence_blocks",
            "barcode_counters",
            "sku_counters",
            "idempotency_keys",
            "import_batches",
            "system_installations",
        ];
        for t in tables {
            let _ = sqlx::query(&format!("DELETE FROM {t}")).execute(pool).await;
        }
        println!("✅ SQLite tables cleared.");
    }

    // Clean generated_documents directory
    let gen_docs_dir = Path::new(&config.generated_documents_dir);
    let target_dir = if gen_docs_dir.exists() {
        gen_docs_dir.to_path_buf()
    } else if Path::new("generated_documents").exists() {
        Path::new("generated_documents").to_path_buf()
    } else {
        Path::new("../backend/generated_documents").to_path_buf()
    };

    if target_dir.exists() {
        println!(
            "🧹 Cleaning generated documents directory: {:?}",
            target_dir
        );
        if let Ok(entries) = fs::read_dir(&target_dir) {
            let mut count = 0;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                    count += 1;
                } else if path.is_file() {
                    let _ = fs::remove_file(&path);
                    count += 1;
                }
            }
            println!("✅ Removed {} generated document entries.", count);
        }
    }

    println!("\n🌱 Seeding base system data (seed_all)...");
    match seeds::seed_all(&db).await {
        Ok(summary) => {
            println!("✅ Master seeding completed successfully!");
            println!("👤 Admin: {}", summary.admin.message);
            println!(
                "📂 Providers: {} categories, {} subcategories created",
                summary.providers.categories_created, summary.providers.subcategories_created
            );
            println!(
                "🏢 Suppliers: {} created",
                summary.suppliers.suppliers_created
            );
            println!(
                "👥 Customers: {} created",
                summary.customers.customers_created
            );
            println!(
                "📦 Inventory: {} categories, {} subcategories, {} products created",
                summary.inventory.categories_created,
                summary.inventory.subcategories_created,
                summary.inventory.products_created
            );
            println!("🔑 API Key: key={}", summary.api_key.key);
        }
        Err(err) => panic!("❌ Failed to seed database: {err}"),
    }

    println!("============================================================");
    println!("✨ Database reset and initial seed completed!");
    println!("============================================================");
}
