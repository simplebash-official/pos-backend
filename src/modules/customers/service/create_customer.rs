use super::sanitize_string::sanitize_string;
use super::sanitize_tags::sanitize_tags;
use super::validate_email::validate_email;
use super::validate_phone_number::validate_phone_number;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::customers::{CreateCustomerRequest, Customer};
use crate::modules::customers::model::CustomerDocument;
use crate::modules::customers::repository;

use mongodb::bson::DateTime as BsonDateTime;

pub async fn create_customer(
    db: &Db,
    body: CreateCustomerRequest,
    device_id: Option<String>,
) -> AppResult<Customer> {
    crate::core::logging::domain::tracked("customers.created", async move {
        let name = body.name.trim().to_string();
        if name.chars().count() < 2 {
            return Err(AppError::validation(
                "Customer name must be at least 2 characters",
            ));
        }
        validate_phone_number(&body.primary_phone, "Primary phone")?;
        let primary_phone = body.primary_phone.trim().to_string();

        let contact_person = sanitize_string(body.contact_person);
        let secondary_phone = sanitize_string(body.secondary_phone);
        if let Some(ref sec_phone) = secondary_phone {
            validate_phone_number(sec_phone, "Secondary phone")?;
        }

        let email = sanitize_string(body.email);
        if let Some(ref e) = email {
            validate_email(e)?;
        }

        let address = sanitize_string(body.address);
        let notes = sanitize_string(body.notes);
        let tags = sanitize_tags(body.tags);

        let key = generate_id(prefixes::CUSTOMER);
        let now = BsonDateTime::now();

        let document = CustomerDocument {
            id: None,
            key,
            name,
            contact_person,
            primary_phone,
            secondary_phone,
            email,
            address,
            tags,
            notes,
            outstanding_balance_cents: 0,
            total_purchases_cents: 0,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: device_id,
        };

        let inserted = repository::insert_customer(db, document).await?;
        Ok(inserted.into_customer())
    })
    .await
}
