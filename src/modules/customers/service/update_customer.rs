use super::sanitize_string::sanitize_string;
use super::sanitize_tags::sanitize_tags;
use super::validate_email::validate_email;
use super::validate_phone_number::validate_phone_number;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::customers::{Customer, UpdateCustomerRequest};

use crate::modules::customers::repository;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn update_customer(
    db: &Db,
    id_or_key: &str,
    body: UpdateCustomerRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Customer> {
    crate::core::logging::domain::tracked("customers.updated", async move {
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

        let name = match body.name {
            Some(ref n) => {
                let trimmed = n.trim().to_string();
                if trimmed.chars().count() < 2 {
                    return Err(AppError::validation(
                        "Customer name must be at least 2 characters",
                    ));
                }
                trimmed
            }
            None => existing.name,
        };

        let primary_phone = match body.primary_phone {
            Some(ref p) => {
                validate_phone_number(p, "Primary phone")?;
                p.trim().to_string()
            }
            None => existing.primary_phone,
        };

        let contact_person = match body.contact_person {
            Some(cp) => sanitize_string(Some(cp)),
            None => existing.contact_person,
        };

        let secondary_phone = match body.secondary_phone {
            Some(sp) => {
                let sanitized = sanitize_string(Some(sp));
                if let Some(ref s) = sanitized {
                    validate_phone_number(s, "Secondary phone")?;
                }
                sanitized
            }
            None => existing.secondary_phone,
        };

        let email = match body.email {
            Some(e) => {
                let sanitized = sanitize_string(Some(e));
                if let Some(ref mail) = sanitized {
                    validate_email(mail)?;
                }
                sanitized
            }
            None => existing.email,
        };

        let address = match body.address {
            Some(a) => sanitize_string(Some(a)),
            None => existing.address,
        };

        let notes = match body.notes {
            Some(n) => sanitize_string(Some(n)),
            None => existing.notes,
        };

        let tags = match body.tags {
            Some(t) => sanitize_tags(t),
            None => existing.tags,
        };

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
