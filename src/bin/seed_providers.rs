use jana2u_pos_backend::{
    clients,
    core::{config::Config, constants::prefixes, id::generate_id},
    modules::inventory::model::CategoryDocument,
};
use mongodb::bson::doc;

/// Seeds the `categories` collection with the inventory module's default
/// category/subcategory reference data. Upserts by name, so it's safe to
/// re-run against a database that already has categories.
/// Run with: `cargo run --bin seed_providers`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    let collection = db.collection::<mongodb::bson::Document>("categories");

    for category in default_categories() {
        let CategoryDocument {
            key,
            name,
            icon,
            color,
            subcategories,
            ..
        } = category;

        // Check if existing document already has a key
        let existing = collection
            .find_one(doc! { "name": &name })
            .await
            .expect("failed to query category");

        let key_to_set = match existing {
            Some(doc) => {
                if let Ok(existing_key) = doc.get_str("key") {
                    if existing_key.is_empty() {
                        key
                    } else {
                        existing_key.to_string()
                    }
                } else {
                    key
                }
            }
            None => key,
        };

        collection
            .update_one(
                doc! { "name": &name },
                doc! {
                    "$set": {
                        "key": key_to_set,
                        "icon": icon,
                        "color": color,
                        "subcategories": subcategories
                    }
                },
            )
            .upsert(true)
            .await
            .expect("failed to upsert category");

        println!("seeded category: {name}");
    }
}

fn default_categories() -> Vec<CategoryDocument> {
    vec![
        CategoryDocument {
            id: None,
            key: generate_id(prefixes::CATEGORY),
            name: "Phone Repairs".to_string(),
            icon: "DeviceMobile".to_string(),
            color: "blue".to_string(),
            subcategories: vec![
                "Phone Covers".to_string(),
                "Screens".to_string(),
                "Batteries".to_string(),
                "Charging Ports".to_string(),
                "Other internal repair parts".to_string(),
            ],
        },
        CategoryDocument {
            id: None,
            key: generate_id(prefixes::CATEGORY),
            name: "Mug, T-Shirt & Print Customization".to_string(),
            icon: "Shirt".to_string(),
            color: "grape".to_string(),
            subcategories: vec![
                "Blank Mugs".to_string(),
                "T-Shirts".to_string(),
                "Sheets (for custom transfers)".to_string(),
                "Sublimation Ink".to_string(),
            ],
        },
        CategoryDocument {
            id: None,
            key: generate_id(prefixes::CATEGORY),
            name: "General Printing".to_string(),
            icon: "Printer".to_string(),
            color: "teal".to_string(),
            subcategories: vec![
                "Paper (documents, photocopies, handbills, and flyers)".to_string(),
                "Printer Ink".to_string(),
            ],
        },
    ]
}
