use jana2u_pos_backend::{
    clients,
    core::{config::Config, constants::prefixes, id::generate_id},
};
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Seeds the `categories` and `subcategories` collections with the
/// inventory module's default reference data. Upserts by name (category
/// by `name`, subcategory by `(category_key, name)`), so it's safe to
/// re-run against a database that already has categories/subcategories.
/// Run with: `cargo run --bin seed_providers`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    let categories = db.collection::<mongodb::bson::Document>("categories");
    let subcategories = db.collection::<mongodb::bson::Document>("subcategories");

    for category in default_categories() {
        let SeedCategory {
            key,
            name,
            icon,
            color,
            subcategory_names,
        } = category;

        // Upserting must not overwrite an already-assigned `key` on re-run
        // (it's meant to be stable), so look up any existing document's key
        // first and only fall back to a freshly generated one if it's
        // missing (a legacy doc from before `key` existed) or genuinely new.
        let existing = categories
            .find_one(doc! { "name": &name })
            .await
            .expect("failed to query category");

        let category_key = match existing {
            Some(doc) => match doc.get_str("key") {
                Ok(existing_key) if !existing_key.is_empty() => existing_key.to_string(),
                _ => key,
            },
            None => key,
        };

        let now = BsonDateTime::now();
        categories
            .update_one(
                doc! { "name": &name },
                doc! {
                    "$set": {
                        "key": &category_key,
                        "icon": icon,
                        "color": color,
                        "updated_at": now
                    },
                    "$setOnInsert": {
                        "created_at": now
                    }
                },
            )
            .upsert(true)
            .await
            .expect("failed to upsert category");

        println!("seeded category: {name}");

        for subcategory_name in subcategory_names {
            let existing_subcategory = subcategories
                .find_one(doc! { "category_key": &category_key, "name": &subcategory_name })
                .await
                .expect("failed to query subcategory");

            let subcategory_key = match existing_subcategory {
                Some(doc) => match doc.get_str("key") {
                    Ok(existing_key) if !existing_key.is_empty() => existing_key.to_string(),
                    _ => generate_id(prefixes::SUBCATEGORY),
                },
                None => generate_id(prefixes::SUBCATEGORY),
            };

            subcategories
                .update_one(
                    doc! { "category_key": &category_key, "name": &subcategory_name },
                    doc! {
                        "$set": {
                            "key": subcategory_key,
                            "updated_at": now
                        },
                        "$setOnInsert": {
                            "created_at": now
                        }
                    },
                )
                .upsert(true)
                .await
                .expect("failed to upsert subcategory");

            println!("  seeded subcategory: {subcategory_name}");
        }
    }
}

/// A bag of fields for the hardcoded starter data below — deliberately not
/// `CategoryDocument`/`SubcategoryDocument` themselves, since this seed
/// script writes to two collections per entry (a category plus its list of
/// subcategory names) rather than persisting one document as-is.
struct SeedCategory {
    key: String,
    name: String,
    icon: String,
    color: String,
    subcategory_names: Vec<String>,
}

/// The shop's fixed starter category/subcategory set. Hardcoded here rather
/// than read from a config file since it changes rarely and only via a
/// deliberate code change + re-run of this binary.
fn default_categories() -> Vec<SeedCategory> {
    vec![
        SeedCategory {
            key: generate_id(prefixes::CATEGORY),
            name: "Phone Repairs".to_string(),
            icon: "DeviceMobile".to_string(),
            color: "blue".to_string(),
            subcategory_names: vec![
                "Phone Covers".to_string(),
                "Screens".to_string(),
                "Batteries".to_string(),
                "Charging Ports".to_string(),
                "Other internal repair parts".to_string(),
            ],
        },
        SeedCategory {
            key: generate_id(prefixes::CATEGORY),
            name: "Mug, T-Shirt & Print Customization".to_string(),
            icon: "Shirt".to_string(),
            color: "grape".to_string(),
            subcategory_names: vec![
                "Blank Mugs".to_string(),
                "T-Shirts".to_string(),
                "Sheets (for custom transfers)".to_string(),
                "Sublimation Ink".to_string(),
            ],
        },
        SeedCategory {
            key: generate_id(prefixes::CATEGORY),
            name: "General Printing".to_string(),
            icon: "Printer".to_string(),
            color: "teal".to_string(),
            subcategory_names: vec![
                "Paper (documents, photocopies, handbills, and flyers)".to_string(),
                "Printer Ink".to_string(),
            ],
        },
    ]
}
