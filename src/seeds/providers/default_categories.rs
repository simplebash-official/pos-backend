use super::seed_category::SeedCategory;

#[must_use]
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
