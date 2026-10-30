use super::resolve_and_validate_contacts::resolve_and_validate_contacts;
use super::sanitize_string::sanitize_string;

use super::validate_email::validate_email;
use super::validate_required_fields::validate_required_fields;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::suppliers::{CreateSupplierRequest, Supplier};

use crate::modules::suppliers::repository;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Full replace for `PUT /suppliers/{id}` — every field in `body` is
/// authoritative (including the genuinely-optional ones), so an omitted
/// `secondaryPhone`/`email`/`notes` clears that field rather than leaving
/// the previous value in place. Contrast with `update_supplier` (PATCH),
/// where an omitted optional field is left untouched.
pub(crate) async fn replace_supplier(
    db: &Db,
    id: ObjectId,
    body: CreateSupplierRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Supplier> {
    crate::core::logging::domain::tracked("suppliers.replaced", async move {
        let existing = repository::find_supplier_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
            })?;

        let (contact_persons, contact_person, primary_phone, secondary_phone) =
            resolve_and_validate_contacts(
                body.contact_persons,
                body.contact_person,
                body.primary_phone,
                body.secondary_phone,
            )?;

        let email = sanitize_string(body.email);
        let notes = sanitize_string(body.notes);

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            let mut conflicting = Vec::new();
            if body.name.trim() != existing.name {
                conflicting.push("name");
            }
            if contact_person != existing.contact_person {
                conflicting.push("contactPerson");
            }
            if primary_phone != existing.primary_phone {
                conflicting.push("primaryPhone");
            }
            if secondary_phone != existing.secondary_phone {
                conflicting.push("secondaryPhone");
            }
            if contact_persons != existing.contact_persons {
                conflicting.push("contactPersons");
            }
            if body.address.trim() != existing.address {
                conflicting.push("address");
            }
            if email != existing.email {
                conflicting.push("email");
            }
            if body.supplied_categories != existing.supplied_categories {
                conflicting.push("suppliedCategories");
            }
            if notes != existing.notes {
                conflicting.push("notes");
            }

            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This supplier was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": existing.into_supplier(),
                    "conflictingFields": conflicting,
                }),
            ));
        }

        validate_required_fields(
            &body.name,
            &contact_person,
            &primary_phone,
            &body.address,
            &body.supplied_categories,
        )?;
        if let Some(ref e) = email {
            validate_email(e)?;
        }

        let mut set_doc = doc! {
            "name": body.name.trim(),
            "contact_person": &contact_person,
            "primary_phone": &primary_phone,
            "address": body.address.trim(),
            "supplied_categories": &body.supplied_categories,
            "updated_at": BsonDateTime::now(),
        };
        if let Ok(cp_bson) = mongodb::bson::serialize_to_bson(&contact_persons) {
            set_doc.insert("contact_persons", cp_bson);
        }
        if let Some(ref sp) = secondary_phone {
            set_doc.insert("secondary_phone", sp);
        } else {
            set_doc.insert("secondary_phone", mongodb::bson::Bson::Null);
        }
        if let Some(e) = email {
            set_doc.insert("email", e);
        } else {
            set_doc.insert("email", mongodb::bson::Bson::Null);
        }
        if let Some(n) = notes {
            set_doc.insert("notes", n);
        } else {
            set_doc.insert("notes", mongodb::bson::Bson::Null);
        }
        if let Some(device) = device_id {
            set_doc.insert("updated_by_device", device);
        }

        let updated = repository::update_supplier(db, id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
            })?;

        Ok(updated.into_supplier())
    })
    .await
}
