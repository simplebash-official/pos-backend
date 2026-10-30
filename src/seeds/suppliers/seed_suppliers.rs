use super::sample_suppliers::sample_suppliers;
use super::supplier_seed_result::SupplierSeedResult;
use crate::clients::db::Db;
use crate::core::error::AppError;
use crate::domain::suppliers::{ContactPerson, CreateSupplierRequest, SupplierListQuery};
use crate::modules::suppliers::service;

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

        let contact_persons = sample
            .contact_persons
            .into_iter()
            .map(|cp| ContactPerson {
                name: cp.name.to_string(),
                phone_numbers: cp.phone_numbers.into_iter().map(String::from).collect(),
            })
            .collect();

        let req = CreateSupplierRequest {
            name: sample.name.to_string(),
            contact_person: Some(sample.contact_person.to_string()),
            primary_phone: Some(sample.primary_phone.to_string()),
            secondary_phone: sample.secondary_phone.map(String::from),
            contact_persons: Some(contact_persons),
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
