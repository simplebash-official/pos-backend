use super::sanitize_string::sanitize_string;
use super::validate_contact_persons::validate_contact_persons;
use super::validate_email::validate_email;
use super::validate_phone::validate_phone;
use super::validate_required_fields::validate_required_fields;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::suppliers::{ContactPerson, Supplier, UpdateSupplierRequest};

use crate::modules::suppliers::repository;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Partial update for `PATCH /suppliers/{id}` — every field in `body` is
/// optional; required fields (name/contactPerson/primaryPhone/address/
/// suppliedCategories) fall back to the existing document's value before
/// re-validation, while genuinely-optional fields
/// (secondaryPhone/email/notes) are only touched when explicitly provided
/// (same convention as `inventory::service::product::update_product`'s
/// `barcode` handling) — an omitted one keeps whatever value was already
/// stored rather than being cleared.
pub(crate) async fn update_supplier(
    db: &Db,
    id: ObjectId,
    body: UpdateSupplierRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Supplier> {
    crate::core::logging::domain::tracked("suppliers.updated", async move {
        let existing = repository::find_supplier_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
            })?;

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            let mut conflicting = Vec::new();
            if let Some(ref name) = body.name
                && name.trim() != existing.name
            {
                conflicting.push("name");
            }
            if let Some(ref cp) = body.contact_person
                && cp.trim() != existing.contact_person
            {
                conflicting.push("contactPerson");
            }
            if let Some(ref pp) = body.primary_phone
                && pp.trim() != existing.primary_phone
            {
                conflicting.push("primaryPhone");
            }
            if let Some(ref sp) = body.secondary_phone
                && sanitize_string(Some(sp.clone())) != existing.secondary_phone
            {
                conflicting.push("secondaryPhone");
            }
            if let Some(ref cps) = body.contact_persons
                && cps != &existing.contact_persons
            {
                conflicting.push("contactPersons");
            }
            if let Some(ref addr) = body.address
                && addr.trim() != existing.address
            {
                conflicting.push("address");
            }
            if let Some(ref email) = body.email
                && sanitize_string(Some(email.clone())) != existing.email
            {
                conflicting.push("email");
            }
            if let Some(ref cats) = body.supplied_categories
                && cats != &existing.supplied_categories
            {
                conflicting.push("suppliedCategories");
            }
            if let Some(ref notes) = body.notes
                && sanitize_string(Some(notes.clone())) != existing.notes
            {
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

        let name = body
            .name
            .map(|n| n.trim().to_string())
            .unwrap_or(existing.name);
        let address = body
            .address
            .map(|a| a.trim().to_string())
            .unwrap_or(existing.address);
        let supplied_categories = body
            .supplied_categories
            .unwrap_or(existing.supplied_categories);

        let (contact_persons, contact_person, primary_phone, secondary_phone) =
            match body.contact_persons {
                Some(cps) => {
                    validate_contact_persons(&cps)?;
                    let primary_cp = &cps[0];
                    let cp_name = primary_cp.name.trim().to_string();
                    let pp = primary_cp.phone_numbers[0].trim().to_string();
                    let sp = primary_cp
                        .phone_numbers
                        .get(1)
                        .map(|p| p.trim().to_string());
                    (cps, cp_name, pp, sp)
                }
                None => {
                    let cp_name = body
                        .contact_person
                        .map(|cp| cp.trim().to_string())
                        .unwrap_or(existing.contact_person.clone());
                    let pp = body
                        .primary_phone
                        .map(|pp| pp.trim().to_string())
                        .unwrap_or(existing.primary_phone.clone());
                    let sp = match body.secondary_phone {
                        Some(s) => {
                            let sanitized = sanitize_string(Some(s));
                            if let Some(ref phone) = sanitized {
                                validate_phone(phone, "Backup phone")?;
                            }
                            sanitized
                        }
                        None => existing.secondary_phone.clone(),
                    };
                    let mut existing_cps = existing.contact_persons.clone();
                    if existing_cps.is_empty() {
                        let mut phones = vec![pp.clone()];
                        if let Some(ref sec) = sp {
                            phones.push(sec.clone());
                        }
                        existing_cps = vec![ContactPerson {
                            name: cp_name.clone(),
                            phone_numbers: phones,
                        }];
                    } else {
                        existing_cps[0].name = cp_name.clone();
                        let mut phones = vec![pp.clone()];
                        if let Some(ref sec) = sp {
                            phones.push(sec.clone());
                        }
                        existing_cps[0].phone_numbers = phones;
                    }
                    (existing_cps, cp_name, pp, sp)
                }
            };

        validate_required_fields(
            &name,
            &contact_person,
            &primary_phone,
            &address,
            &supplied_categories,
        )?;

        let has_email = body.email.is_some();
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

        let has_notes = body.notes.is_some();
        let notes = match body.notes {
            Some(n) => sanitize_string(Some(n)),
            None => existing.notes,
        };

        let mut set_doc = doc! {
            "name": &name,
            "contact_person": &contact_person,
            "primary_phone": &primary_phone,
            "address": &address,
            "supplied_categories": &supplied_categories,
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
        if has_email {
            if let Some(e) = email {
                set_doc.insert("email", e);
            } else {
                set_doc.insert("email", mongodb::bson::Bson::Null);
            }
        }
        if has_notes {
            if let Some(n) = notes {
                set_doc.insert("notes", n);
            } else {
                set_doc.insert("notes", mongodb::bson::Bson::Null);
            }
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
