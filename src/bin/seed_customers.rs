use jana2u_pos_backend::{
    clients,
    core::{config::Config, constants::prefixes, id::generate_id},
};
use mongodb::bson::doc;

/// Seeds the `customers` collection with 20 realistic sample retail, repair,
/// corporate, and print customers. Upserts by `name`, preserving an already-assigned
/// `key` on re-run (same pattern as `seed_suppliers.rs` and `seed_providers.rs`),
/// so it's safe to re-run against a database that already has customer records.
///
/// Run with: `cargo run --bin seed_customers`
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    let customers = db.collection::<mongodb::bson::Document>("customers");

    for customer in sample_customers() {
        let SeedCustomer {
            key,
            name,
            contact_person,
            primary_phone,
            secondary_phone,
            email,
            address,
            tags,
            notes,
            outstanding_balance_cents,
            total_purchases_cents,
        } = customer;

        let existing = customers
            .find_one(doc! { "name": &name })
            .await
            .expect("failed to query customer");

        let customer_key = match existing {
            Some(ref doc) => match doc.get_str("key") {
                Ok(existing_key) if !existing_key.is_empty() => existing_key.to_string(),
                _ => key,
            },
            None => key,
        };

        let now = mongodb::bson::DateTime::now();
        let mut set_doc = doc! {
            "key": &customer_key,
            "primary_phone": primary_phone,
            "tags": tags,
            "updated_at": now,
        };

        if let Some(cp) = contact_person {
            set_doc.insert("contact_person", cp);
        } else {
            set_doc.insert("contact_person", mongodb::bson::Bson::Null);
        }

        if let Some(sp) = secondary_phone {
            set_doc.insert("secondary_phone", sp);
        } else {
            set_doc.insert("secondary_phone", mongodb::bson::Bson::Null);
        }

        if let Some(em) = email {
            set_doc.insert("email", em);
        } else {
            set_doc.insert("email", mongodb::bson::Bson::Null);
        }

        if let Some(addr) = address {
            set_doc.insert("address", addr);
        } else {
            set_doc.insert("address", mongodb::bson::Bson::Null);
        }

        if let Some(n) = notes {
            set_doc.insert("notes", n);
        } else {
            set_doc.insert("notes", mongodb::bson::Bson::Null);
        }

        let set_on_insert = doc! {
            "created_at": now,
            "version": 1i64,
            "outstanding_balance_cents": outstanding_balance_cents,
            "total_purchases_cents": total_purchases_cents,
        };

        // If not already in DB, initialize financial totals and created_at
        customers
            .update_one(
                doc! { "name": &name },
                doc! {
                    "$set": set_doc,
                    "$setOnInsert": set_on_insert,
                },
            )
            .upsert(true)
            .await
            .expect("failed to upsert customer");

        println!("seeded customer: {name} ({customer_key})");
    }

    println!("\nSuccessfully seeded 20 customers into the database.");
}

struct SeedCustomer {
    key: String,
    name: String,
    contact_person: Option<String>,
    primary_phone: String,
    secondary_phone: Option<String>,
    email: Option<String>,
    address: Option<String>,
    tags: Vec<String>,
    notes: Option<String>,
    outstanding_balance_cents: i64,
    total_purchases_cents: i64,
}

fn sample_customers() -> Vec<SeedCustomer> {
    vec![
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Saman Perera".to_string(),
            contact_person: Some("Saman Perera".to_string()),
            primary_phone: "0771234567".to_string(),
            secondary_phone: Some("0112345678".to_string()),
            email: Some("saman@example.com".to_string()),
            address: Some("No. 12, Galle Road, Colombo 03".to_string()),
            tags: vec!["Retail Client".to_string(), "VIP Customer".to_string()],
            notes: Some("Prefers SMS updates.".to_string()),
            outstanding_balance_cents: 150000,
            total_purchases_cents: 8500000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "ABC Enterprises".to_string(),
            contact_person: Some("Kavinda Fernando".to_string()),
            primary_phone: "0114567890".to_string(),
            secondary_phone: Some("0719876543".to_string()),
            email: Some("purchasing@abcenterprises.lk".to_string()),
            address: Some("Level 4, Millennium Tower, Colombo 02".to_string()),
            tags: vec!["Corporate Account".to_string(), "Wholesale".to_string()],
            notes: Some("30-day credit period.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 14500000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Nimali Jayawardena".to_string(),
            contact_person: None,
            primary_phone: "0718899001".to_string(),
            secondary_phone: None,
            email: Some("nimali.j@gmail.com".to_string()),
            address: Some("45/2, Kandy Road, Kiribathgoda".to_string()),
            tags: vec!["Repair Client".to_string()],
            notes: Some("Frequent phone screen repairs.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 1250000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Apex Tech Solutions".to_string(),
            contact_person: Some("Dinesh Weerasinghe".to_string()),
            primary_phone: "0112334455".to_string(),
            secondary_phone: Some("0772211009".to_string()),
            email: Some("info@apextech.lk".to_string()),
            address: Some("102, High Level Road, Nugegoda".to_string()),
            tags: vec!["Corporate Account".to_string(), "Print Client".to_string()],
            notes: Some("Bulk monthly invoice printing.".to_string()),
            outstanding_balance_cents: 450000,
            total_purchases_cents: 9200000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Kamal Gunaratne".to_string(),
            contact_person: None,
            primary_phone: "0765544332".to_string(),
            secondary_phone: None,
            email: None,
            address: Some("No. 7, Temple Road, Maharagama".to_string()),
            tags: vec!["Retail Client".to_string()],
            notes: None,
            outstanding_balance_cents: 0,
            total_purchases_cents: 350000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Dilshan Senanayake".to_string(),
            contact_person: None,
            primary_phone: "0779988776".to_string(),
            secondary_phone: Some("0112876543".to_string()),
            email: Some("dilshan.s@outlook.com".to_string()),
            address: Some("18/A, Havelock Road, Colombo 05".to_string()),
            tags: vec!["VIP Customer".to_string(), "Repair Client".to_string()],
            notes: Some("VIP discount 10% on accessories.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 4800000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Lanka Creatives Studio".to_string(),
            contact_person: Some("Anusha Wickramasinghe".to_string()),
            primary_phone: "0117788990".to_string(),
            secondary_phone: None,
            email: Some("hello@lankacreatives.com".to_string()),
            address: Some("88, Duplication Road, Colombo 04".to_string()),
            tags: vec!["Print Client".to_string(), "Wholesale".to_string()],
            notes: Some("T-shirt and mug printing orders.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 6700000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Ruwan Bandara".to_string(),
            contact_person: None,
            primary_phone: "0701122334".to_string(),
            secondary_phone: None,
            email: None,
            address: None,
            tags: vec!["Retail Client".to_string()],
            notes: Some("Quick create walk-in customer.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 180000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Kandy Auto Spares".to_string(),
            contact_person: Some("Mohamed Rizwan".to_string()),
            primary_phone: "0812233445".to_string(),
            secondary_phone: Some("0778811223".to_string()),
            email: Some("rizwan@kandyautospares.lk".to_string()),
            address: Some("25, Peradeniya Road, Kandy".to_string()),
            tags: vec!["Wholesale".to_string(), "Corporate Account".to_string()],
            notes: Some("Stationery and billing paper purchaser.".to_string()),
            outstanding_balance_cents: 220000,
            total_purchases_cents: 5100000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Tharindu Rathnayake".to_string(),
            contact_person: None,
            primary_phone: "0723344556".to_string(),
            secondary_phone: None,
            email: Some("tharindu.r@yahoo.com".to_string()),
            address: Some("64, Negombo Road, Wattala".to_string()),
            tags: vec!["Repair Client".to_string()],
            notes: Some("Laptop battery and display replacement.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 2900000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Blue Horizon Travels".to_string(),
            contact_person: Some("Priyantha Silva".to_string()),
            primary_phone: "0119900112".to_string(),
            secondary_phone: None,
            email: Some("accounts@bluehorizon.lk".to_string()),
            address: Some("50, Dharmapala Mawatha, Colombo 07".to_string()),
            tags: vec!["Corporate Account".to_string(), "Print Client".to_string()],
            notes: Some("Brochures and marketing print jobs.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 11200000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Ishara Madushanka".to_string(),
            contact_person: None,
            primary_phone: "0756677889".to_string(),
            secondary_phone: None,
            email: Some("ishara.m@gmail.com".to_string()),
            address: Some("12, Station Road, Dehiwala".to_string()),
            tags: vec!["Retail Client".to_string()],
            notes: None,
            outstanding_balance_cents: 0,
            total_purchases_cents: 450000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Global Logistics Lanka".to_string(),
            contact_person: Some("Sanduni Perera".to_string()),
            primary_phone: "0114455667".to_string(),
            secondary_phone: Some("0773322110".to_string()),
            email: Some("procurement@globallogistics.lk".to_string()),
            address: Some("77, Baseline Road, Colombo 09".to_string()),
            tags: vec!["Corporate Account".to_string()],
            notes: Some("Official courier invoice printing.".to_string()),
            outstanding_balance_cents: 380000,
            total_purchases_cents: 8900000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Kasun Wickremasooriya".to_string(),
            contact_person: None,
            primary_phone: "0773344112".to_string(),
            secondary_phone: None,
            email: Some("kasun.w@gmail.com".to_string()),
            address: Some("23/1, Ward Place, Colombo 07".to_string()),
            tags: vec!["VIP Customer".to_string(), "Retail Client".to_string()],
            notes: Some("Regular buyer of premium phone accessories.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 3400000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Rasika Jayasuriya".to_string(),
            contact_person: None,
            primary_phone: "0789900112".to_string(),
            secondary_phone: None,
            email: None,
            address: Some("89, Main Street, Gampaha".to_string()),
            tags: vec!["Repair Client".to_string()],
            notes: Some("iPad screen replacement warranty active.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 1750000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Sun & Moon Cafe".to_string(),
            contact_person: Some("Hafiz Ahamed".to_string()),
            primary_phone: "0112244668".to_string(),
            secondary_phone: None,
            email: Some("info@sunandmooncafe.lk".to_string()),
            address: Some("14, Park Road, Colombo 05".to_string()),
            tags: vec!["Print Client".to_string()],
            notes: Some("Monthly menu cards and banner printing.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 2800000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Nuwan Pradeep".to_string(),
            contact_person: None,
            primary_phone: "0714455667".to_string(),
            secondary_phone: None,
            email: Some("nuwan.p@hotmail.com".to_string()),
            address: Some("55, Old Kottawa Road, Pannipitiya".to_string()),
            tags: vec!["Retail Client".to_string()],
            notes: None,
            outstanding_balance_cents: 0,
            total_purchases_cents: 620000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "NextGen IT Academy".to_string(),
            contact_person: Some("Chathura Disanayake".to_string()),
            primary_phone: "0118899112".to_string(),
            secondary_phone: Some("0712233445".to_string()),
            email: Some("admin@nextgen.lk".to_string()),
            address: Some("310, Galle Road, Moratuwa".to_string()),
            tags: vec!["Corporate Account".to_string(), "Print Client".to_string()],
            notes: Some("Student ID card and certificate printing.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 7400000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "Sachini Alwis".to_string(),
            contact_person: None,
            primary_phone: "0778899223".to_string(),
            secondary_phone: None,
            email: None,
            address: Some("10, Flower Road, Colombo 07".to_string()),
            tags: vec!["Retail Client".to_string()],
            notes: Some("Prefers WhatsApp receipts.".to_string()),
            outstanding_balance_cents: 0,
            total_purchases_cents: 950000,
        },
        SeedCustomer {
            key: generate_id(prefixes::CUSTOMER),
            name: "City Medical Center".to_string(),
            contact_person: Some("Dr. Rohana Wijesinghe".to_string()),
            primary_phone: "0115566778".to_string(),
            secondary_phone: Some("0774433221".to_string()),
            email: Some("rohana@citymedical.lk".to_string()),
            address: Some("120, Cotta Road, Borella".to_string()),
            tags: vec!["Corporate Account".to_string(), "Print Client".to_string()],
            notes: Some("Prescription pad and medical report printing.".to_string()),
            outstanding_balance_cents: 520000,
            total_purchases_cents: 16500000,
        },
    ]
}
