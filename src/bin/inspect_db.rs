use simplebash_pos_backend::{
    clients,
    core::config::Config,
    domain::inventory::ProductListQuery,
    modules::inventory::service::{category, product},
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("invalid configuration");
    let db = clients::db::connect_from_config(&config)
        .await
        .expect("failed to connect to database");

    let categories_res = category::list_categories(&db)
        .await
        .expect("failed to list categories");

    let mut total_products = 0;
    let mut subcategories_count = 0;

    println!(
        "Total Categories found: {}",
        categories_res.categories.len()
    );
    for cat in &categories_res.categories {
        println!("\nCategory: '{}' (key: {})", cat.name, cat.key);
        for subcat in &cat.subcategories {
            subcategories_count += 1;
            let prods = product::list_products(
                &db,
                ProductListQuery {
                    category_key: Some(cat.key.clone()),
                    subcategory_key: Some(subcat.key.clone()),
                    limit: Some(500),
                    ..Default::default()
                },
            )
            .await
            .expect("failed to list products");

            println!(
                "   Subcategory: '{}' (key: {}) -> {} products",
                subcat.name,
                subcat.key,
                prods.items.len()
            );
            total_products += prods.items.len();
        }
    }
    println!("\nTotal Subcategories: {}", subcategories_count);
    println!("Total Products in DB: {}", total_products);
}
