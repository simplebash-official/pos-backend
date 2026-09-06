use crate::{
    clients::db::Db,
    core::error::AppError,
    domain::inventory::{CreateCategoryRequest, UpdateCategoryRequest},
    modules::inventory::service::category,
};

#[derive(Debug, Clone)]
pub struct ProviderSeedResult {
    pub categories_created: usize,
    pub subcategories_created: usize,
    pub categories_existing: usize,
}

pub struct SeedCategory {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: &'static str,
    pub subcategory_names: Vec<&'static str>,
}

pub fn default_categories() -> Vec<SeedCategory> {
    vec![
        SeedCategory {
            name: "Phone Repairs",
            icon: "DeviceMobile",
            color: "blue",
            subcategory_names: vec![
                "Phone Covers",
                "Screens",
                "Batteries",
                "Charging Ports",
                "Other internal repair parts",
            ],
        },
        SeedCategory {
            name: "Mug, T-Shirt & Print Customization",
            icon: "Shirt",
            color: "grape",
            subcategory_names: vec![
                "Blank Mugs",
                "T-Shirts",
                "Sheets (for custom transfers)",
                "Sublimation Ink",
            ],
        },
        SeedCategory {
            name: "General Printing",
            icon: "Printer",
            color: "teal",
            subcategory_names: vec![
                "Paper (documents, photocopies, handbills, and flyers)",
                "Printer Ink",
            ],
        },
    ]
}

pub async fn seed_providers(db: &Db) -> Result<ProviderSeedResult, AppError> {
    let mut categories_created = 0;
    let mut subcategories_created = 0;
    let mut categories_existing = 0;

    for cat in default_categories() {
        let existing_categories = category::list_categories(db).await?;
        let existing = existing_categories
            .categories
            .into_iter()
            .find(|c| c.name == cat.name);

        let category_info = match existing {
            Some(existing) => {
                categories_existing += 1;
                if existing.icon != cat.icon || existing.color != cat.color {
                    category::update_category(
                        db,
                        existing.key.clone(),
                        UpdateCategoryRequest {
                            name: None,
                            icon: Some(cat.icon.to_string()),
                            color: Some(cat.color.to_string()),
                        },
                    )
                    .await?
                } else {
                    existing
                }
            }
            None => {
                let req = CreateCategoryRequest {
                    name: cat.name.to_string(),
                    icon: cat.icon.to_string(),
                    color: cat.color.to_string(),
                    subcategories: cat
                        .subcategory_names
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                };
                let created = category::create_category(db, req).await?;
                categories_created += 1;
                created
            }
        };

        for subcat_name in cat.subcategory_names {
            let has_subcat = category_info
                .subcategories
                .iter()
                .any(|s| s.name == subcat_name);
            if !has_subcat {
                category::add_subcategory(db, category_info.key.clone(), subcat_name.to_string())
                    .await?;
                subcategories_created += 1;
            }
        }
    }

    Ok(ProviderSeedResult {
        categories_created,
        subcategories_created,
        categories_existing,
    })
}
