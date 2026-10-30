use super::sanitize_string::sanitize_string;
use super::sanitize_tags::sanitize_tags;

use super::validate_email::validate_email;
use super::validate_phone_number::validate_phone_number;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::customers::{CreateCustomerRequest, Customer};

use crate::modules::customers::repository;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn replace_customer(
    db: &Db,
    id_or_key: &str,
    body: CreateCustomerRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Customer> {
    crate::core::logging::domain::tracked("customers.replaced", async move {
        let existing = repository::find_customer_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
            })?;

        let object_id = existing
            .id
            .expect("persisted customer document must have an _id");

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This customer was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": existing.into_customer(),
                }),
            ));
        }

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

        let mut set_doc = doc! {
            "name": &name,
            "primary_phone": &primary_phone,
            "tags": &tags,
            "updated_at": BsonDateTime::now(),
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

        if let Some(device) = device_id {
            set_doc.insert("updated_by_device", device);
        }

        let updated = repository::update_customer(db, object_id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
            })?;

        Ok(updated.into_customer())
    })
    .await
}
