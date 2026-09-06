use crate::{
    clients::db::Db,
    core::error::AppError,
    domain::suppliers::{CreateSupplierRequest, SupplierListQuery},
    modules::suppliers::service,
};

#[derive(Debug, Clone)]
pub struct SupplierSeedResult {
    pub suppliers_created: usize,
    pub suppliers_existing: usize,
}

pub struct SeedSupplier {
    pub name: &'static str,
    pub contact_person: &'static str,
    pub primary_phone: &'static str,
    pub secondary_phone: Option<&'static str>,
    pub address: &'static str,
    pub supplied_categories: Vec<&'static str>,
    pub email: Option<&'static str>,
    pub notes: Option<&'static str>,
}

pub fn sample_suppliers() -> Vec<SeedSupplier> {
    vec![
        SeedSupplier {
            name: "Colombo Mobile Parts",
            contact_person: "Ranjith Silva",
            primary_phone: "077 123 4567",
            secondary_phone: Some("011 234 5678"),
            address: "No. 45, First Cross Street, Pettah, Colombo 11",
            supplied_categories: vec![
                "Phone Parts",
                "Display Assemblies",
                "Batteries",
                "Repair Tools",
            ],
            email: Some("sales@colombomobileparts.lk"),
            notes: Some("Preferred supplier for iPhone and Samsung original displays."),
        },
        SeedSupplier {
            name: "Negombo Screen Traders",
            contact_person: "Priyantha Fernando",
            primary_phone: "071 456 7890",
            secondary_phone: None,
            address: "No. 12, Poruthota Road, Negombo",
            supplied_categories: vec!["Phone Repairs", "Screens", "Charging Ports"],
            email: Some("info@negomboscreens.lk"),
            notes: Some("Delivers to Colombo shops every Monday and Thursday."),
        },
        SeedSupplier {
            name: "Galle Sublimation Hub",
            contact_person: "Chamari Perera",
            primary_phone: "076 234 5678",
            secondary_phone: Some("091 222 3344"),
            address: "No. 88, Matara Road, Galle",
            supplied_categories: vec!["Mug Blanks", "T-Shirts", "Sublimation Ink"],
            email: Some("orders@gallesublimation.lk"),
            notes: Some("Bulk pricing available for orders over 100 units."),
        },
        SeedSupplier {
            name: "Kandy Print Supplies",
            contact_person: "Nuwan Bandara",
            primary_phone: "081 345 6789",
            secondary_phone: None,
            address: "No. 23, Peradeniya Road, Kandy",
            supplied_categories: vec!["Paper", "Printer Ink", "General Printing"],
            email: None,
            notes: Some("Cash on delivery only; no card payments accepted."),
        },
        SeedSupplier {
            name: "Jaffna Repair Tools & Parts",
            contact_person: "Kumaran Selvarajah",
            primary_phone: "070 987 6543",
            secondary_phone: Some("021 222 5566"),
            address: "No. 5, Hospital Road, Jaffna",
            supplied_categories: vec!["Repair Tools", "Batteries", "Charging Ports"],
            email: Some("contact@jaffnarepairtools.lk"),
            notes: None,
        },
    ]
}

pub async fn seed_suppliers(db: &Db) -> Result<SupplierSeedResult, AppError> {
    let mut suppliers_created = 0;
    let mut suppliers_existing = 0;

    let existing_list = service::list_suppliers(db, SupplierListQuery::default()).await?;

    for sample in sample_suppliers() {
        if existing_list
            .suppliers
            .iter()
            .any(|s| s.name == sample.name)
        {
            suppliers_existing += 1;
            continue;
        }

        let req = CreateSupplierRequest {
            name: sample.name.to_string(),
            contact_person: sample.contact_person.to_string(),
            primary_phone: sample.primary_phone.to_string(),
            secondary_phone: sample.secondary_phone.map(String::from),
            address: sample.address.to_string(),
            supplied_categories: sample
                .supplied_categories
                .iter()
                .map(|s| s.to_string())
                .collect(),
            email: sample.email.map(String::from),
            notes: sample.notes.map(String::from),
        };

        service::create_supplier(db, req).await?;
        suppliers_created += 1;
    }

    Ok(SupplierSeedResult {
        suppliers_created,
        suppliers_existing,
    })
}
