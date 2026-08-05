use jana2u_pos_backend::{
    clients,
    core::config::Config,
    domain::inventory::{CreateCategoryRequest, CreateProductRequest, ProductListQuery},
    modules::inventory::service::{category, product},
};

struct SeedProduct {
    name: &'static str,
    cost_price_cents: i64,
    selling_price_cents: i64,
    stock_quantity: i64,
    min_stock_threshold: i64,
    barcode: &'static str,
}

struct SeedSubcategory {
    name: &'static str,
    products: Vec<SeedProduct>,
}

struct SeedCategory {
    name: &'static str,
    icon: &'static str,
    color: &'static str,
    subcategories: Vec<SeedSubcategory>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    println!("🌱 Starting inventory seeding via codebase services...");

    let seed_data = get_seed_data();
    let mut total_categories = 0;
    let mut total_subcategories = 0;
    let mut total_products = 0;

    for cat in seed_data {
        let subcat_names: Vec<String> = cat.subcategories.iter().map(|s| s.name.to_string()).collect();

        // Check if category already exists via category service
        let existing_categories = category::list_categories(&db)
            .await
            .expect("failed to list categories");
        
        let category_info = match existing_categories.categories.into_iter().find(|c| c.name == cat.name) {
            Some(existing) => {
                println!("📦 Category '{}' already exists (key: {})", existing.name, existing.key);
                existing
            }
            None => {
                let req = CreateCategoryRequest {
                    name: cat.name.to_string(),
                    icon: cat.icon.to_string(),
                    color: cat.color.to_string(),
                    subcategories: subcat_names,
                };
                let created = category::create_category(&db, req)
                    .await
                    .expect("failed to create category via service");
                println!("✅ Created Category '{}' (key: {})", created.name, created.key);
                total_categories += 1;
                created
            }
        };

        // Map subcategory names to keys, creating missing ones if necessary via category service
        for subcat in cat.subcategories {
            let existing_sub = category_info.subcategories.iter().find(|s| s.name == subcat.name);
            let subcat_key = match existing_sub {
                Some(s) => s.key.clone(),
                None => {
                    let updated_cat = category::add_subcategory(&db, category_info.key.clone(), subcat.name.to_string())
                        .await
                        .expect("failed to add subcategory via service");
                    let created_sub = updated_cat
                        .subcategories
                        .into_iter()
                        .find(|s| s.name == subcat.name)
                        .expect("subcategory should exist after creation");
                    println!("  ➡️ Added Subcategory '{}' (key: {})", created_sub.name, created_sub.key);
                    total_subcategories += 1;
                    created_sub.key
                }
            };

            // Fetch existing products in this subcategory to avoid duplicating on re-run
            let existing_products = product::list_products(
                &db,
                ProductListQuery {
                    category_key: Some(category_info.key.clone()),
                    subcategory_key: Some(subcat_key.clone()),
                    page: Some(1),
                    limit: Some(200),
                    ..Default::default()
                },
            )
            .await
            .expect("failed to list products");

            for p in subcat.products {
                let exists = existing_products.items.iter().any(|item| item.name == p.name);
                if exists {
                    println!("    🔹 Product '{}' already exists", p.name);
                    continue;
                }

                let req = CreateProductRequest {
                    barcode: Some(p.barcode.to_string()),
                    name: p.name.to_string(),
                    category_key: category_info.key.clone(),
                    subcategory_key: subcat_key.clone(),
                    cost_price_cents: p.cost_price_cents,
                    selling_price_cents: p.selling_price_cents,
                    stock_quantity: p.stock_quantity,
                    min_stock_threshold: p.min_stock_threshold,
                };

                let created_product = product::create_product(&db, req)
                    .await
                    .expect("failed to create product via service");

                println!(
                    "    🛒 Created Product '{}' | SKU: {} | Stock: {} | Price: ${:.2}",
                    created_product.name,
                    created_product.sku,
                    created_product.stock_quantity,
                    created_product.selling_price_cents as f64 / 100.0
                );
                total_products += 1;
            }
        }
    }

    println!("\n🎉 Seeding complete!");
    println!("   New categories created: {}", total_categories);
    println!("   New subcategories created: {}", total_subcategories);
    println!("   New products created: {}", total_products);
}

fn get_seed_data() -> Vec<SeedCategory> {
    vec![
        // 1. Smartphones & Accessories
        SeedCategory {
            name: "Smartphones & Accessories",
            icon: "DeviceMobile",
            color: "blue",
            subcategories: vec![
                SeedSubcategory {
                    name: "Screen Protectors",
                    products: vec![
                        SeedProduct { name: "iPhone 15 Pro Max Tempered Glass", cost_price_cents: 500, selling_price_cents: 1500, stock_quantity: 50, min_stock_threshold: 10, barcode: "880123456701" },
                        SeedProduct { name: "Samsung Galaxy S24 Ultra UV Glass", cost_price_cents: 650, selling_price_cents: 1800, stock_quantity: 40, min_stock_threshold: 8, barcode: "880123456702" },
                        SeedProduct { name: "Privacy Screen Guard iPhone 14", cost_price_cents: 450, selling_price_cents: 1400, stock_quantity: 30, min_stock_threshold: 5, barcode: "880123456703" },
                    ],
                },
                SeedSubcategory {
                    name: "Phone Cases",
                    products: vec![
                        SeedProduct { name: "MagSafe Clear Case iPhone 15", cost_price_cents: 800, selling_price_cents: 2500, stock_quantity: 35, min_stock_threshold: 10, barcode: "880123456704" },
                        SeedProduct { name: "Heavy Duty Armor Case Galaxy S23", cost_price_cents: 1000, selling_price_cents: 2800, stock_quantity: 25, min_stock_threshold: 5, barcode: "880123456705" },
                        SeedProduct { name: "Silicone Soft Cover Redmi Note 13", cost_price_cents: 300, selling_price_cents: 1000, stock_quantity: 45, min_stock_threshold: 10, barcode: "880123456706" },
                    ],
                },
                SeedSubcategory {
                    name: "Charging & Cables",
                    products: vec![
                        SeedProduct { name: "20W USB-C Fast Charger Block", cost_price_cents: 750, selling_price_cents: 2000, stock_quantity: 60, min_stock_threshold: 15, barcode: "880123456707" },
                        SeedProduct { name: "Braided USB-C to USB-C Cable 2m", cost_price_cents: 350, selling_price_cents: 1200, stock_quantity: 80, min_stock_threshold: 20, barcode: "880123456708" },
                        SeedProduct { name: "3-in-1 Magnetic Wireless Station", cost_price_cents: 2200, selling_price_cents: 4999, stock_quantity: 20, min_stock_threshold: 5, barcode: "880123456709" },
                    ],
                },
                SeedSubcategory {
                    name: "Replacement Batteries",
                    products: vec![
                        SeedProduct { name: "High Capacity Battery for iPhone 11 (3110mAh)", cost_price_cents: 1200, selling_price_cents: 3500, stock_quantity: 20, min_stock_threshold: 5, barcode: "880123456710" },
                        SeedProduct { name: "Replacement Battery for Samsung A52", cost_price_cents: 1100, selling_price_cents: 3200, stock_quantity: 15, min_stock_threshold: 5, barcode: "880123456711" },
                        SeedProduct { name: "Battery Pack for iPad Air 4", cost_price_cents: 1800, selling_price_cents: 5000, stock_quantity: 10, min_stock_threshold: 3, barcode: "880123456712" },
                    ],
                },
                SeedSubcategory {
                    name: "Audio Accessories",
                    products: vec![
                        SeedProduct { name: "Wireless Earbuds TWS Pro", cost_price_cents: 1500, selling_price_cents: 4500, stock_quantity: 30, min_stock_threshold: 8, barcode: "880123456713" },
                        SeedProduct { name: "3.5mm AUX to Lightning Adapter", cost_price_cents: 250, selling_price_cents: 999, stock_quantity: 50, min_stock_threshold: 10, barcode: "880123456714" },
                        SeedProduct { name: "Bluetooth Neckband Headphones", cost_price_cents: 900, selling_price_cents: 2499, stock_quantity: 25, min_stock_threshold: 5, barcode: "880123456715" },
                    ],
                },
            ],
        },
        // 2. Computer & Laptop Parts
        SeedCategory {
            name: "Computer & Laptop Parts",
            icon: "Laptop",
            color: "indigo",
            subcategories: vec![
                SeedSubcategory {
                    name: "Storage Drives",
                    products: vec![
                        SeedProduct { name: "NVMe M.2 SSD 1TB High-Speed", cost_price_cents: 4500, selling_price_cents: 8500, stock_quantity: 25, min_stock_threshold: 5, barcode: "880123456716" },
                        SeedProduct { name: "2.5-inch SATA SSD 512GB", cost_price_cents: 2500, selling_price_cents: 4800, stock_quantity: 30, min_stock_threshold: 8, barcode: "880123456717" },
                        SeedProduct { name: "External Portable Hard Drive 2TB", cost_price_cents: 5000, selling_price_cents: 8999, stock_quantity: 15, min_stock_threshold: 4, barcode: "880123456718" },
                    ],
                },
                SeedSubcategory {
                    name: "RAM Modules",
                    products: vec![
                        SeedProduct { name: "DDR4 16GB 3200MHz Laptop SODIMM", cost_price_cents: 2800, selling_price_cents: 5200, stock_quantity: 40, min_stock_threshold: 10, barcode: "880123456719" },
                        SeedProduct { name: "DDR5 32GB 5600MHz Desktop RAM", cost_price_cents: 7000, selling_price_cents: 12500, stock_quantity: 15, min_stock_threshold: 3, barcode: "880123456720" },
                        SeedProduct { name: "DDR4 8GB 2666MHz Desktop RAM", cost_price_cents: 1400, selling_price_cents: 2999, stock_quantity: 35, min_stock_threshold: 8, barcode: "880123456721" },
                    ],
                },
                SeedSubcategory {
                    name: "Keyboards & Mice",
                    products: vec![
                        SeedProduct { name: "Wireless Ergonomic Mouse", cost_price_cents: 1200, selling_price_cents: 2999, stock_quantity: 45, min_stock_threshold: 10, barcode: "880123456722" },
                        SeedProduct { name: "Mechanical Gaming Keyboard RGB", cost_price_cents: 3000, selling_price_cents: 6999, stock_quantity: 20, min_stock_threshold: 5, barcode: "880123456723" },
                        SeedProduct { name: "Slim Silent Wireless Combo", cost_price_cents: 1800, selling_price_cents: 3999, stock_quantity: 25, min_stock_threshold: 5, barcode: "880123456724" },
                    ],
                },
                SeedSubcategory {
                    name: "Networking Gear",
                    products: vec![
                        SeedProduct { name: "Dual-Band Wi-Fi 6 Router", cost_price_cents: 3500, selling_price_cents: 7500, stock_quantity: 18, min_stock_threshold: 4, barcode: "880123456725" },
                        SeedProduct { name: "USB 3.0 Gigabit Ethernet Adapter", cost_price_cents: 800, selling_price_cents: 1999, stock_quantity: 40, min_stock_threshold: 10, barcode: "880123456726" },
                        SeedProduct { name: "CAT6 RJ45 Patch Cable 5m", cost_price_cents: 200, selling_price_cents: 699, stock_quantity: 100, min_stock_threshold: 20, barcode: "880123456727" },
                    ],
                },
                SeedSubcategory {
                    name: "Laptop Power Adapters",
                    products: vec![
                        SeedProduct { name: "65W USB-C Universal Laptop Charger", cost_price_cents: 1600, selling_price_cents: 3500, stock_quantity: 30, min_stock_threshold: 6, barcode: "880123456728" },
                        SeedProduct { name: "90W AC Adapter for Dell/HP", cost_price_cents: 1400, selling_price_cents: 3200, stock_quantity: 20, min_stock_threshold: 5, barcode: "880123456729" },
                        SeedProduct { name: "MagSafe 2 85W Power Adapter", cost_price_cents: 2200, selling_price_cents: 4999, stock_quantity: 15, min_stock_threshold: 3, barcode: "880123456730" },
                    ],
                },
            ],
        },
        // 3. Custom Printing & Gifts
        SeedCategory {
            name: "Custom Printing & Gifts",
            icon: "Shirt",
            color: "grape",
            subcategories: vec![
                SeedSubcategory {
                    name: "Blank Drinkware",
                    products: vec![
                        SeedProduct { name: "11oz White Ceramic Sublimation Mug", cost_price_cents: 120, selling_price_cents: 500, stock_quantity: 150, min_stock_threshold: 30, barcode: "880123456731" },
                        SeedProduct { name: "Inner Color Ceramic Mug (Black)", cost_price_cents: 180, selling_price_cents: 700, stock_quantity: 100, min_stock_threshold: 20, barcode: "880123456732" },
                        SeedProduct { name: "Stainless Steel Travel Tumbler 20oz", cost_price_cents: 550, selling_price_cents: 1600, stock_quantity: 60, min_stock_threshold: 15, barcode: "880123456733" },
                    ],
                },
                SeedSubcategory {
                    name: "Apparel Blanks",
                    products: vec![
                        SeedProduct { name: "100% Cotton T-Shirt (Black - XL)", cost_price_cents: 350, selling_price_cents: 1200, stock_quantity: 80, min_stock_threshold: 20, barcode: "880123456734" },
                        SeedProduct { name: "Sublimation T-Shirt (White - L)", cost_price_cents: 300, selling_price_cents: 1000, stock_quantity: 100, min_stock_threshold: 25, barcode: "880123456735" },
                        SeedProduct { name: "Unisex Fleece Hoodie (Navy Blue - M)", cost_price_cents: 1200, selling_price_cents: 3200, stock_quantity: 40, min_stock_threshold: 10, barcode: "880123456736" },
                    ],
                },
                SeedSubcategory {
                    name: "Custom Signage & Banners",
                    products: vec![
                        SeedProduct { name: "Roll-up Banner Stand with Canvas (33x81)", cost_price_cents: 1800, selling_price_cents: 5500, stock_quantity: 15, min_stock_threshold: 3, barcode: "880123456737" },
                        SeedProduct { name: "Acrylic Desk Name Plate Blank", cost_price_cents: 400, selling_price_cents: 1500, stock_quantity: 35, min_stock_threshold: 8, barcode: "880123456738" },
                        SeedProduct { name: "Vinyl Sticker Sheet A4 (Pack of 50)", cost_price_cents: 600, selling_price_cents: 2000, stock_quantity: 50, min_stock_threshold: 10, barcode: "880123456739" },
                    ],
                },
                SeedSubcategory {
                    name: "Sublimation Supplies",
                    products: vec![
                        SeedProduct { name: "Sublimation Ink Set CMYK (100ml x 4)", cost_price_cents: 1500, selling_price_cents: 3999, stock_quantity: 25, min_stock_threshold: 5, barcode: "880123456740" },
                        SeedProduct { name: "Heat Resistant Tape Roll 10mm", cost_price_cents: 100, selling_price_cents: 450, stock_quantity: 120, min_stock_threshold: 30, barcode: "880123456741" },
                        SeedProduct { name: "Sublimation Transfer Paper A4 (100 Sheets)", cost_price_cents: 700, selling_price_cents: 1999, stock_quantity: 45, min_stock_threshold: 10, barcode: "880123456742" },
                    ],
                },
                SeedSubcategory {
                    name: "Photo Gifts",
                    products: vec![
                        SeedProduct { name: "Custom Crystal Photo Block Blank", cost_price_cents: 800, selling_price_cents: 2499, stock_quantity: 30, min_stock_threshold: 5, barcode: "880123456743" },
                        SeedProduct { name: "Sublimation Keyring Blank (Rectangular)", cost_price_cents: 50, selling_price_cents: 300, stock_quantity: 200, min_stock_threshold: 40, barcode: "880123456744" },
                        SeedProduct { name: "Custom Printed Mouse Pad Blank", cost_price_cents: 150, selling_price_cents: 650, stock_quantity: 90, min_stock_threshold: 20, barcode: "880123456745" },
                    ],
                },
            ],
        },
        // 4. Office & Paper Stationery
        SeedCategory {
            name: "Office & Paper Stationery",
            icon: "Printer",
            color: "teal",
            subcategories: vec![
                SeedSubcategory {
                    name: "Printing Paper",
                    products: vec![
                        SeedProduct { name: "A4 Copy Paper 80gsm (Box of 5 Reams)", cost_price_cents: 1800, selling_price_cents: 3200, stock_quantity: 40, min_stock_threshold: 10, barcode: "880123456746" },
                        SeedProduct { name: "Glossy Photo Paper A4 230gsm (50 sheets)", cost_price_cents: 450, selling_price_cents: 1299, stock_quantity: 60, min_stock_threshold: 15, barcode: "880123456747" },
                        SeedProduct { name: "Thermal Receipt Paper Rolls 80x80mm (Pack of 10)", cost_price_cents: 800, selling_price_cents: 1999, stock_quantity: 50, min_stock_threshold: 10, barcode: "880123456748" },
                    ],
                },
                SeedSubcategory {
                    name: "Inks & Toners",
                    products: vec![
                        SeedProduct { name: "Black Toner Cartridge HP 85A Compatible", cost_price_cents: 1200, selling_price_cents: 2999, stock_quantity: 30, min_stock_threshold: 5, barcode: "880123456749" },
                        SeedProduct { name: "Epson EcoTank Black Ink Bottle 664", cost_price_cents: 600, selling_price_cents: 1499, stock_quantity: 45, min_stock_threshold: 10, barcode: "880123456750" },
                        SeedProduct { name: "Canon CLI-751 CMYK Ink Set", cost_price_cents: 2200, selling_price_cents: 4999, stock_quantity: 20, min_stock_threshold: 5, barcode: "880123456751" },
                    ],
                },
                SeedSubcategory {
                    name: "Filing & Binding",
                    products: vec![
                        SeedProduct { name: "A4 Lever Arch File Folder (Blue)", cost_price_cents: 180, selling_price_cents: 500, stock_quantity: 80, min_stock_threshold: 20, barcode: "880123456752" },
                        SeedProduct { name: "Plastic Comb Binding Spines 12mm (Pack of 100)", cost_price_cents: 350, selling_price_cents: 999, stock_quantity: 40, min_stock_threshold: 10, barcode: "880123456753" },
                        SeedProduct { name: "Transparent Presentation Covers A4 (100 Pack)", cost_price_cents: 400, selling_price_cents: 1199, stock_quantity: 35, min_stock_threshold: 8, barcode: "880123456754" },
                    ],
                },
                SeedSubcategory {
                    name: "Writing Instruments",
                    products: vec![
                        SeedProduct { name: "Gel Ink Pen 0.5mm Black (Box of 12)", cost_price_cents: 300, selling_price_cents: 850, stock_quantity: 70, min_stock_threshold: 15, barcode: "880123456755" },
                        SeedProduct { name: "Permanent Marker Set (Black/Blue/Red)", cost_price_cents: 220, selling_price_cents: 600, stock_quantity: 90, min_stock_threshold: 20, barcode: "880123456756" },
                        SeedProduct { name: "Highlighter Assorted Colors (Pack of 4)", cost_price_cents: 150, selling_price_cents: 499, stock_quantity: 100, min_stock_threshold: 25, barcode: "880123456757" },
                    ],
                },
                SeedSubcategory {
                    name: "Desktop Supplies",
                    products: vec![
                        SeedProduct { name: "Heavy Duty Desk Stapler & Staples Set", cost_price_cents: 500, selling_price_cents: 1400, stock_quantity: 35, min_stock_threshold: 8, barcode: "880123456758" },
                        SeedProduct { name: "Desktop Tape Dispenser with 2 Tape Rolls", cost_price_cents: 250, selling_price_cents: 750, stock_quantity: 50, min_stock_threshold: 10, barcode: "880123456759" },
                        SeedProduct { name: "Steel Mesh Document Tray 3-Tier", cost_price_cents: 700, selling_price_cents: 1899, stock_quantity: 25, min_stock_threshold: 5, barcode: "880123456760" },
                    ],
                },
            ],
        },
        // 5. Gaming & Gadgets
        SeedCategory {
            name: "Gaming & Gadgets",
            icon: "DeviceGamepad",
            color: "orange",
            subcategories: vec![
                SeedSubcategory {
                    name: "Gaming Controllers",
                    products: vec![
                        SeedProduct { name: "Wireless Gamepad for PC & Android", cost_price_cents: 1600, selling_price_cents: 3500, stock_quantity: 30, min_stock_threshold: 8, barcode: "880123456761" },
                        SeedProduct { name: "Dual Vibration Console Controller", cost_price_cents: 2200, selling_price_cents: 4999, stock_quantity: 25, min_stock_threshold: 5, barcode: "880123456762" },
                        SeedProduct { name: "Mobile Gaming Joystick Controller Grip", cost_price_cents: 450, selling_price_cents: 1299, stock_quantity: 40, min_stock_threshold: 10, barcode: "880123456763" },
                    ],
                },
                SeedSubcategory {
                    name: "Headsets & Microphones",
                    products: vec![
                        SeedProduct { name: "RGB Gaming Headset with Noise-Canceling Mic", cost_price_cents: 1800, selling_price_cents: 4200, stock_quantity: 35, min_stock_threshold: 8, barcode: "880123456764" },
                        SeedProduct { name: "USB Condenser Microphone with Stand", cost_price_cents: 2500, selling_price_cents: 5999, stock_quantity: 20, min_stock_threshold: 5, barcode: "880123456765" },
                        SeedProduct { name: "Gaming Headphone Stand with USB Hub", cost_price_cents: 900, selling_price_cents: 2250, stock_quantity: 30, min_stock_threshold: 6, barcode: "880123456766" },
                    ],
                },
                SeedSubcategory {
                    name: "Action Cameras & Accessories",
                    products: vec![
                        SeedProduct { name: "4K Action Camera Waterproof 60FPS", cost_price_cents: 3500, selling_price_cents: 7999, stock_quantity: 15, min_stock_threshold: 4, barcode: "880123456767" },
                        SeedProduct { name: "Chest Mount Strap for Action Camera", cost_price_cents: 400, selling_price_cents: 1200, stock_quantity: 45, min_stock_threshold: 10, barcode: "880123456768" },
                        SeedProduct { name: "Flexible Mini Tripod Gorilla Pod", cost_price_cents: 350, selling_price_cents: 999, stock_quantity: 60, min_stock_threshold: 12, barcode: "880123456769" },
                    ],
                },
                SeedSubcategory {
                    name: "Smart Watch Accessories",
                    products: vec![
                        SeedProduct { name: "Silicone Sport Band 20mm (Black)", cost_price_cents: 200, selling_price_cents: 799, stock_quantity: 80, min_stock_threshold: 15, barcode: "880123456770" },
                        SeedProduct { name: "Magnetic Metal Mesh Strap for Apple Watch", cost_price_cents: 450, selling_price_cents: 1499, stock_quantity: 50, min_stock_threshold: 10, barcode: "880123456771" },
                        SeedProduct { name: "Screen Protector Glass for Galaxy Watch 6", cost_price_cents: 150, selling_price_cents: 599, stock_quantity: 90, min_stock_threshold: 20, barcode: "880123456772" },
                    ],
                },
                SeedSubcategory {
                    name: "LED Lighting & Strips",
                    products: vec![
                        SeedProduct { name: "RGB LED Strip Light 5m with Remote", cost_price_cents: 600, selling_price_cents: 1799, stock_quantity: 50, min_stock_threshold: 10, barcode: "880123456773" },
                        SeedProduct { name: "USB Ring Light 10-inch with Phone Holder", cost_price_cents: 850, selling_price_cents: 2499, stock_quantity: 35, min_stock_threshold: 8, barcode: "880123456774" },
                        SeedProduct { name: "Smart RGB Corner Floor Lamp", cost_price_cents: 2400, selling_price_cents: 5999, stock_quantity: 15, min_stock_threshold: 3, barcode: "880123456775" },
                    ],
                },
            ],
        },
    ]
}
