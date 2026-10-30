use super::customer_seed_result::CustomerSeedResult;
use super::sample_customers::sample_customers;
use crate::clients::db::Db;
use crate::core::error::AppError;
use crate::domain::customers::{CreateCustomerRequest, CustomerListQuery};
use crate::modules::customers::service;

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
