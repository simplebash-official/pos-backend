use crate::{
    clients::db::Db,
    core::error::AppError,
    domain::customers::{CreateCustomerRequest, CustomerListQuery},
    modules::customers::service,
};

#[derive(Debug, Clone)]
pub struct CustomerSeedResult {
    pub customers_created: usize,
    pub customers_existing: usize,
}

pub struct SeedCustomer {
    pub name: &'static str,
    pub contact_person: Option<&'static str>,
    pub primary_phone: &'static str,
    pub secondary_phone: Option<&'static str>,
    pub email: Option<&'static str>,
    pub address: Option<&'static str>,
    pub tags: Vec<&'static str>,
    pub notes: Option<&'static str>,
    pub outstanding_balance_cents: i64,
    pub total_purchases_cents: i64,
}

pub fn sample_customers() -> Vec<SeedCustomer> {
    vec![
        SeedCustomer {
            name: "Saman Perera",
            contact_person: Some("Saman Perera"),
            primary_phone: "0771234567",
            secondary_phone: Some("0112345678"),
            email: Some("saman@example.com"),
            address: Some("No. 12, Galle Road, Colombo 03"),
            tags: vec!["Retail Client", "VIP Customer"],
            notes: Some("Prefers SMS updates."),
            outstanding_balance_cents: 150000,
            total_purchases_cents: 8500000,
        },
        SeedCustomer {
            name: "ABC Enterprises",
            contact_person: Some("Kavinda Fernando"),
            primary_phone: "0114567890",
            secondary_phone: Some("0719876543"),
            email: Some("purchasing@abcenterprises.lk"),
            address: Some("Level 4, Millennium Tower, Colombo 02"),
            tags: vec!["Corporate Account", "Wholesale"],
            notes: Some("30-day credit period."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 14500000,
        },
        SeedCustomer {
            name: "Nimali Jayawardena",
            contact_person: None,
            primary_phone: "0718899001",
            secondary_phone: None,
            email: Some("nimali.j@gmail.com"),
            address: Some("45/2, Kandy Road, Kiribathgoda"),
            tags: vec!["Repair Client"],
            notes: Some("Frequent phone screen repairs."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 1250000,
        },
        SeedCustomer {
            name: "Apex Tech Solutions",
            contact_person: Some("Dinesh Weerasinghe"),
            primary_phone: "0112334455",
            secondary_phone: Some("0772211009"),
            email: Some("info@apextech.lk"),
            address: Some("102, High Level Road, Nugegoda"),
            tags: vec!["Corporate Account", "Print Client"],
            notes: Some("Bulk monthly invoice printing."),
            outstanding_balance_cents: 450000,
            total_purchases_cents: 9200000,
        },
        SeedCustomer {
            name: "Kamal Gunaratne",
            contact_person: None,
            primary_phone: "0765544332",
            secondary_phone: None,
            email: None,
            address: Some("No. 7, Temple Road, Maharagama"),
            tags: vec!["Retail Client"],
            notes: None,
            outstanding_balance_cents: 0,
            total_purchases_cents: 350000,
        },
        SeedCustomer {
            name: "Dilshan Senanayake",
            contact_person: None,
            primary_phone: "0779988776",
            secondary_phone: Some("0112876543"),
            email: Some("dilshan.s@outlook.com"),
            address: Some("18/A, Havelock Road, Colombo 05"),
            tags: vec!["VIP Customer", "Repair Client"],
            notes: Some("VIP discount 10% on accessories."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 4800000,
        },
        SeedCustomer {
            name: "Lanka Creatives Studio",
            contact_person: Some("Anusha Wickramasinghe"),
            primary_phone: "0117788990",
            secondary_phone: None,
            email: Some("hello@lankacreatives.com"),
            address: Some("88, Duplication Road, Colombo 04"),
            tags: vec!["Print Client", "Wholesale"],
            notes: Some("T-shirt and mug printing orders."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 6700000,
        },
        SeedCustomer {
            name: "Ruwan Bandara",
            contact_person: None,
            primary_phone: "0701122334",
            secondary_phone: None,
            email: None,
            address: None,
            tags: vec!["Retail Client"],
            notes: Some("Quick create walk-in customer."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 180000,
        },
        SeedCustomer {
            name: "Kandy Auto Spares",
            contact_person: Some("Mohamed Rizwan"),
            primary_phone: "0812233445",
            secondary_phone: Some("0778811223"),
            email: Some("rizwan@kandyautospares.lk"),
            address: Some("25, Peradeniya Road, Kandy"),
            tags: vec!["Wholesale", "Corporate Account"],
            notes: Some("Stationery and billing paper purchaser."),
            outstanding_balance_cents: 220000,
            total_purchases_cents: 5100000,
        },
        SeedCustomer {
            name: "Tharindu Rathnayake",
            contact_person: None,
            primary_phone: "0723344556",
            secondary_phone: None,
            email: Some("tharindu.r@yahoo.com"),
            address: Some("64, Negombo Road, Wattala"),
            tags: vec!["Repair Client"],
            notes: Some("Laptop battery and display replacement."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 2900000,
        },
        SeedCustomer {
            name: "Blue Horizon Travels",
            contact_person: Some("Priyantha Silva"),
            primary_phone: "0119900112",
            secondary_phone: None,
            email: Some("accounts@bluehorizon.lk"),
            address: Some("50, Dharmapala Mawatha, Colombo 07"),
            tags: vec!["Corporate Account", "Print Client"],
            notes: Some("Brochures and marketing print jobs."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 11200000,
        },
        SeedCustomer {
            name: "Ishara Madushanka",
            contact_person: None,
            primary_phone: "0756677889",
            secondary_phone: None,
            email: Some("ishara.m@gmail.com"),
            address: Some("12, Station Road, Dehiwala"),
            tags: vec!["Retail Client"],
            notes: None,
            outstanding_balance_cents: 0,
            total_purchases_cents: 450000,
        },
        SeedCustomer {
            name: "Global Logistics Lanka",
            contact_person: Some("Sanduni Perera"),
            primary_phone: "0114455667",
            secondary_phone: Some("0773322110"),
            email: Some("procurement@globallogistics.lk"),
            address: Some("77, Baseline Road, Colombo 09"),
            tags: vec!["Corporate Account"],
            notes: Some("Official courier invoice printing."),
            outstanding_balance_cents: 380000,
            total_purchases_cents: 8900000,
        },
        SeedCustomer {
            name: "Kasun Wickremasooriya",
            contact_person: None,
            primary_phone: "0773344112",
            secondary_phone: None,
            email: Some("kasun.w@gmail.com"),
            address: Some("23/1, Ward Place, Colombo 07"),
            tags: vec!["VIP Customer", "Retail Client"],
            notes: Some("Regular buyer of premium phone accessories."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 3400000,
        },
        SeedCustomer {
            name: "Rasika Jayasuriya",
            contact_person: None,
            primary_phone: "0789900112",
            secondary_phone: None,
            email: None,
            address: Some("89, Main Street, Gampaha"),
            tags: vec!["Repair Client"],
            notes: Some("iPad screen replacement warranty active."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 1750000,
        },
        SeedCustomer {
            name: "Sun & Moon Cafe",
            contact_person: Some("Hafiz Ahamed"),
            primary_phone: "0112244668",
            secondary_phone: None,
            email: Some("info@sunandmooncafe.lk"),
            address: Some("14, Park Road, Colombo 05"),
            tags: vec!["Print Client"],
            notes: Some("Monthly menu cards and banner printing."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 2800000,
        },
        SeedCustomer {
            name: "Nuwan Pradeep",
            contact_person: None,
            primary_phone: "0714455667",
            secondary_phone: None,
            email: Some("nuwan.p@hotmail.com"),
            address: Some("55, Old Kottawa Road, Pannipitiya"),
            tags: vec!["Retail Client"],
            notes: None,
            outstanding_balance_cents: 0,
            total_purchases_cents: 620000,
        },
        SeedCustomer {
            name: "NextGen IT Academy",
            contact_person: Some("Chathura Disanayake"),
            primary_phone: "0118899112",
            secondary_phone: Some("0712233445"),
            email: Some("admin@nextgen.lk"),
            address: Some("310, Galle Road, Moratuwa"),
            tags: vec!["Corporate Account", "Print Client"],
            notes: Some("Student ID card and certificate printing."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 7400000,
        },
        SeedCustomer {
            name: "Sachini Alwis",
            contact_person: None,
            primary_phone: "0778899223",
            secondary_phone: None,
            email: None,
            address: Some("10, Flower Road, Colombo 07"),
            tags: vec!["Retail Client"],
            notes: Some("Prefers WhatsApp receipts."),
            outstanding_balance_cents: 0,
            total_purchases_cents: 950000,
        },
        SeedCustomer {
            name: "City Medical Center",
            contact_person: Some("Dr. Rohana Wijesinghe"),
            primary_phone: "0115566778",
            secondary_phone: Some("0774433221"),
            email: Some("rohana@citymedical.lk"),
            address: Some("120, Cotta Road, Borella"),
            tags: vec!["Corporate Account", "Print Client"],
            notes: Some("Prescription pad and medical report printing."),
            outstanding_balance_cents: 520000,
            total_purchases_cents: 16500000,
        },
    ]
}

pub async fn seed_customers(db: &Db) -> Result<CustomerSeedResult, AppError> {
    let mut customers_created = 0;
    let mut customers_existing = 0;

    let existing_list = service::list_customers(
        db,
        CustomerListQuery {
            page: Some(1),
            limit: Some(100),
            ..Default::default()
        },
    )
    .await?;

    for sample in sample_customers() {
        if existing_list
            .customers
            .iter()
            .any(|c| c.name == sample.name || c.primary_phone == sample.primary_phone)
        {
            customers_existing += 1;
            continue;
        }

        let req = CreateCustomerRequest {
            name: sample.name.to_string(),
            contact_person: sample.contact_person.map(String::from),
            primary_phone: sample.primary_phone.to_string(),
            secondary_phone: sample.secondary_phone.map(String::from),
            email: sample.email.map(String::from),
            address: sample.address.map(String::from),
            tags: sample.tags.iter().map(|s| s.to_string()).collect(),
            notes: sample.notes.map(String::from),
        };

        let created = service::create_customer(db, req, None).await?;
        if sample.outstanding_balance_cents != 0 || sample.total_purchases_cents != 0 {
            service::apply_financial_delta(
                db,
                &created.key,
                sample.total_purchases_cents,
                sample.outstanding_balance_cents,
            )
            .await?;
        }
        customers_created += 1;
    }

    Ok(CustomerSeedResult {
        customers_created,
        customers_existing,
    })
}
