use jana2u_pos_backend::{
    clients,
    core::{config::Config, constants::prefixes, id::generate_id},
};
use mongodb::bson::doc;

/// Seeds the `suppliers` collection with a handful of sample repair/retail
/// suppliers. Upserts by `name`, preserving an already-assigned `key` on
/// re-run (same pattern as `seed_providers.rs`), so it's safe to re-run
/// against a database that already has suppliers.
/// Run with: `cargo run --bin seed_suppliers`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    let suppliers = db.collection::<mongodb::bson::Document>("suppliers");

    for supplier in sample_suppliers() {
        let SeedSupplier {
            key,
            name,
            contact_person,
            primary_phone,
            secondary_phone,
            address,
            supplied_categories,
            email,
            notes,
        } = supplier;

        // Upserting must not overwrite an already-assigned `key` on re-run
        // (it's meant to be stable), so look up any existing document's key
        // first and only fall back to the freshly generated one if it's
        // missing or genuinely new — same approach as `seed_providers.rs`.
        let existing = suppliers
            .find_one(doc! { "name": &name })
            .await
            .expect("failed to query supplier");

        let supplier_key = match existing {
            Some(doc) => match doc.get_str("key") {
                Ok(existing_key) if !existing_key.is_empty() => existing_key.to_string(),
                _ => key,
            },
            None => key,
        };

        let now = mongodb::bson::DateTime::now();
        suppliers
            .update_one(
                doc! { "name": &name },
                doc! {
                    "$set": {
                        "key": &supplier_key,
                        "contact_person": contact_person,
                        "primary_phone": primary_phone,
                        "secondary_phone": secondary_phone,
                        "address": address,
                        "supplied_categories": supplied_categories,
                        "email": email,
                        "notes": notes,
                        "updated_at": now,
                    },
                    "$setOnInsert": {
                        "created_at": now
                    }
                },
            )
            .upsert(true)
            .await
            .expect("failed to upsert supplier");

        println!("seeded supplier: {name}");
    }
}

/// A bag of fields for the hardcoded sample data below — deliberately not
/// `SupplierDocument` itself, since `key`/`created_at` need the same
/// preserve-on-re-run handling `seed_providers.rs` uses for categories.
struct SeedSupplier {
    key: String,
    name: String,
    contact_person: String,
    primary_phone: String,
    secondary_phone: Option<String>,
    address: String,
    supplied_categories: Vec<String>,
    email: Option<String>,
    notes: Option<String>,
}

/// Sample suppliers covering the category/subcategory reference data
/// `seed_providers.rs` seeds (phone repair parts, mug/t-shirt/print
/// customization, general printing), so linked products in a freshly seeded
/// database have realistic supplier coverage. Hardcoded here for the same
/// reason `seed_providers.rs`'s `default_categories` is hardcoded — a small,
/// rarely-changing starter set.
fn sample_suppliers() -> Vec<SeedSupplier> {
    vec![
        SeedSupplier {
            key: generate_id(prefixes::SUPPLIER),
            name: "Colombo Mobile Parts".to_string(),
            contact_person: "Ranjith Silva".to_string(),
            primary_phone: "077 123 4567".to_string(),
            secondary_phone: Some("011 234 5678".to_string()),
            address: "No. 45, First Cross Street, Pettah, Colombo 11".to_string(),
            supplied_categories: vec![
                "Phone Parts".to_string(),
                "Display Assemblies".to_string(),
                "Batteries".to_string(),
                "Repair Tools".to_string(),
            ],
            email: Some("sales@colombomobileparts.lk".to_string()),
            notes: Some("Preferred supplier for iPhone and Samsung original displays.".to_string()),
        },
        SeedSupplier {
            key: generate_id(prefixes::SUPPLIER),
            name: "Negombo Screen Traders".to_string(),
            contact_person: "Priyantha Fernando".to_string(),
            primary_phone: "071 456 7890".to_string(),
            secondary_phone: None,
            address: "No. 12, Poruthota Road, Negombo".to_string(),
            supplied_categories: vec![
                "Phone Repairs".to_string(),
                "Screens".to_string(),
                "Charging Ports".to_string(),
            ],
            email: Some("info@negomboscreens.lk".to_string()),
            notes: Some("Delivers to Colombo shops every Monday and Thursday.".to_string()),
        },
        SeedSupplier {
            key: generate_id(prefixes::SUPPLIER),
            name: "Galle Sublimation Hub".to_string(),
            contact_person: "Chamari Perera".to_string(),
            primary_phone: "076 234 5678".to_string(),
            secondary_phone: Some("091 222 3344".to_string()),
            address: "No. 88, Matara Road, Galle".to_string(),
            supplied_categories: vec![
                "Mug Blanks".to_string(),
                "T-Shirts".to_string(),
                "Sublimation Ink".to_string(),
            ],
            email: Some("orders@gallesublimation.lk".to_string()),
            notes: Some("Bulk pricing available for orders over 100 units.".to_string()),
        },
        SeedSupplier {
            key: generate_id(prefixes::SUPPLIER),
            name: "Kandy Print Supplies".to_string(),
            contact_person: "Nuwan Bandara".to_string(),
            primary_phone: "081 345 6789".to_string(),
            secondary_phone: None,
            address: "No. 23, Peradeniya Road, Kandy".to_string(),
            supplied_categories: vec![
                "Paper".to_string(),
                "Printer Ink".to_string(),
                "General Printing".to_string(),
            ],
            email: None,
            notes: Some("Cash on delivery only; no card payments accepted.".to_string()),
        },
        SeedSupplier {
            key: generate_id(prefixes::SUPPLIER),
            name: "Jaffna Repair Tools & Parts".to_string(),
            contact_person: "Kumaran Selvarajah".to_string(),
            primary_phone: "070 987 6543".to_string(),
            secondary_phone: Some("021 222 5566".to_string()),
            address: "No. 5, Hospital Road, Jaffna".to_string(),
            supplied_categories: vec![
                "Repair Tools".to_string(),
                "Batteries".to_string(),
                "Charging Ports".to_string(),
            ],
            email: Some("contact@jaffnarepairtools.lk".to_string()),
            notes: None,
        },
    ]
}
