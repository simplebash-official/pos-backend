use super::seed_contact_person::SeedContactPerson;
use super::seed_supplier::SeedSupplier;

#[must_use]
pub fn sample_suppliers() -> Vec<SeedSupplier> {
    vec![
        SeedSupplier {
            name: "Colombo Mobile Parts",
            contact_person: "Ranjith Silva",
            primary_phone: "077 123 4567",
            secondary_phone: Some("011 234 5678"),
            contact_persons: vec![
                SeedContactPerson {
                    name: "Ranjith Silva",
                    phone_numbers: vec!["077 123 4567", "011 234 5678"],
                },
                SeedContactPerson {
                    name: "Sunil Perera",
                    phone_numbers: vec!["077 987 6543"],
                },
            ],
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
            contact_persons: vec![
                SeedContactPerson {
                    name: "Priyantha Fernando",
                    phone_numbers: vec!["071 456 7890"],
                },
                SeedContactPerson {
                    name: "Kavinda Wickramasinghe",
                    phone_numbers: vec!["031 222 4455", "071 888 9900"],
                },
            ],
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
            contact_persons: vec![SeedContactPerson {
                name: "Chamari Perera",
                phone_numbers: vec!["076 234 5678", "091 222 3344"],
            }],
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
            contact_persons: vec![SeedContactPerson {
                name: "Nuwan Bandara",
                phone_numbers: vec!["081 345 6789"],
            }],
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
            contact_persons: vec![
                SeedContactPerson {
                    name: "Kumaran Selvarajah",
                    phone_numbers: vec!["070 987 6543", "021 222 5566"],
                },
                SeedContactPerson {
                    name: "Sivaram Thambiah",
                    phone_numbers: vec!["077 555 1234"],
                },
            ],
            address: "No. 5, Hospital Road, Jaffna",
            supplied_categories: vec!["Repair Tools", "Batteries", "Charging Ports"],
            email: Some("contact@jaffnarepairtools.lk"),
            notes: None,
        },
    ]
}
