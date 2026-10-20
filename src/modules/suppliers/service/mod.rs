// Business rules for supplier CRUD/listing: required-field validation,
// lightweight phone/email format checks, and the "still referenced by
// purchase history" guard that blocks deleting a supplier while purchases
// still point at it by key. Delegates all Mongo access to `super::repository`.

use axum::http::StatusCode;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::build_bson_regex,
    },
    domain::suppliers::{
        CreateSupplierRequest, Supplier, SupplierCategoriesResponse, SupplierListQuery,
        SupplierStats, SuppliersResponse, UpdateSupplierRequest,
    },
    modules::suppliers::{model::SupplierDocument, repository},
};

/// Name/contact-person/phone/address/category invariants shared by create,
/// full replace, and (on the merged result) partial update — kept separate
/// from `validate_email` since the latter only applies to a field that's
/// genuinely optional on the entity, not one that's always present.
fn validate_required_fields(
    name: &str,
    contact_person: &str,
    primary_phone: &str,
    address: &str,
    supplied_categories: &[String],
) -> AppResult<()> {
    if name.trim().chars().count() < 2 {
        return Err(AppError::validation(
            "Supplier name must be at least 2 characters",
        ));
    }
    if contact_person.trim().is_empty() {
        return Err(AppError::validation("Contact person is required"));
    }
    if primary_phone.trim().is_empty() {
        return Err(AppError::validation("Primary phone is required"));
    }
    if !primary_phone
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(
            "Primary phone may only contain digits, spaces, dashes, and a plus sign",
        ));
    }
    if address.trim().is_empty() {
        return Err(AppError::validation("Address is required"));
    }
    if supplied_categories.is_empty() {
        return Err(AppError::validation(
            "At least one supplied category is required",
        ));
    }
    Ok(())
}

/// A deliberately lightweight structural check (single `@`, non-empty local
/// part, dotted domain) rather than a full RFC 5322 validator — no `regex`
/// crate dependency exists in this codebase, and this is the same bar as
/// the codebase's other hand-rolled string validation (see
/// `core::utils::regex_escape`).
fn validate_email(email: &str) -> AppResult<()> {
    let is_valid = email.matches('@').count() == 1
        && !email.contains(' ')
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });

    if !is_valid {
        return Err(AppError::validation("Email is not a valid email address"));
    }
    Ok(())
}

fn validate_phone(phone: &str, field_label: &str) -> AppResult<()> {
    if !phone
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(format!(
            "{field_label} may only contain digits, spaces, dashes, and a plus sign"
        )));
    }
    Ok(())
}

fn sanitize_string(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Builds the Mongo filter from query params (free-text search across
/// name/contactPerson/primaryPhone/address, plus a `suppliedCategories`
/// membership filter) — same construction style as
/// `inventory::service::product::list_products`. Not paginated: a shop's
/// supplier list is small enough to return in full.
pub async fn list_suppliers(db: &Db, query: SupplierListQuery) -> AppResult<SuppliersResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = build_bson_regex(&search);
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "contact_person": { "$regex": pattern.clone() } },
                { "primary_phone": { "$regex": pattern.clone() } },
                { "address": { "$regex": pattern } },
            ]
        });
    }
    if let Some(category) = query.category.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "supplied_categories": category });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let documents = repository::list_suppliers(db, filter).await?;
    let suppliers = documents
        .into_iter()
        .map(SupplierDocument::into_supplier)
        .collect();

    Ok(SuppliersResponse { suppliers })
}

/// Fetch by id, 404ing with the module-specific `SUPPLIER_NOT_FOUND` code
/// rather than the generic `NOT_FOUND`.
pub(crate) async fn get_supplier(db: &Db, id: ObjectId) -> AppResult<Supplier> {
    let document = repository::find_supplier_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
        })?;

    Ok(document.into_supplier())
}

/// Same as `get_supplier`, looked up by `key` instead of `ObjectId` — the
/// cross-module entry point `supplier_products`/`purchases` call to
/// validate a `supplierKey` and enrich their own responses.
pub(crate) async fn get_supplier_by_key(db: &Db, key: &str) -> AppResult<Supplier> {
    let document = repository::find_supplier_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
        })?;

    Ok(document.into_supplier())
}

/// Cross-module batch lookup — `purchases::service::purchase::list_purchases`
/// calls this to enrich a page of purchase history with supplier display
/// data in one query instead of one `get_supplier_by_key` call per row.
pub(crate) async fn get_suppliers_by_keys(db: &Db, keys: &[String]) -> AppResult<Vec<Supplier>> {
    let documents = repository::find_suppliers_by_keys(db, keys).await?;
    Ok(documents
        .into_iter()
        .map(SupplierDocument::into_supplier)
        .collect())
}

pub async fn create_supplier(db: &Db, body: CreateSupplierRequest) -> AppResult<Supplier> {
    crate::core::logging::domain::tracked("suppliers.created", async move {
        validate_required_fields(
            &body.name,
            &body.contact_person,
            &body.primary_phone,
            &body.address,
            &body.supplied_categories,
        )?;

        let secondary_phone = sanitize_string(body.secondary_phone);
        if let Some(ref sec_phone) = secondary_phone {
            validate_phone(sec_phone, "Backup phone")?;
        }

        let email = sanitize_string(body.email);
        if let Some(ref e) = email {
            validate_email(e)?;
        }

        let notes = sanitize_string(body.notes);

        let now = BsonDateTime::now();
        let document = SupplierDocument {
            id: None,
            key: generate_id(prefixes::SUPPLIER),
            name: body.name.trim().to_string(),
            contact_person: body.contact_person.trim().to_string(),
            primary_phone: body.primary_phone.trim().to_string(),
            secondary_phone,
            address: body.address.trim().to_string(),
            supplied_categories: body.supplied_categories,
            email,
            notes,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        };

        let inserted = repository::insert_supplier(db, document).await?;
        Ok(inserted.into_supplier())
    })
    .await
}

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

        let secondary_phone = sanitize_string(body.secondary_phone);
        let email = sanitize_string(body.email);
        let notes = sanitize_string(body.notes);

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            let mut conflicting = Vec::new();
            if body.name.trim() != existing.name {
                conflicting.push("name");
            }
            if body.contact_person.trim() != existing.contact_person {
                conflicting.push("contactPerson");
            }
            if body.primary_phone.trim() != existing.primary_phone {
                conflicting.push("primaryPhone");
            }
            if secondary_phone != existing.secondary_phone {
                conflicting.push("secondaryPhone");
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
            &body.contact_person,
            &body.primary_phone,
            &body.address,
            &body.supplied_categories,
        )?;
        if let Some(ref sec_phone) = secondary_phone {
            validate_phone(sec_phone, "Backup phone")?;
        }
        if let Some(ref e) = email {
            validate_email(e)?;
        }

        let mut set_doc = doc! {
            "name": body.name.trim(),
            "contact_person": body.contact_person.trim(),
            "primary_phone": body.primary_phone.trim(),
            "address": body.address.trim(),
            "supplied_categories": &body.supplied_categories,
            "updated_at": BsonDateTime::now(),
        };
        if let Some(sp) = secondary_phone {
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
        let contact_person = body
            .contact_person
            .map(|cp| cp.trim().to_string())
            .unwrap_or(existing.contact_person);
        let primary_phone = body
            .primary_phone
            .map(|pp| pp.trim().to_string())
            .unwrap_or(existing.primary_phone);
        let address = body
            .address
            .map(|a| a.trim().to_string())
            .unwrap_or(existing.address);
        let supplied_categories = body
            .supplied_categories
            .unwrap_or(existing.supplied_categories);

        validate_required_fields(
            &name,
            &contact_person,
            &primary_phone,
            &address,
            &supplied_categories,
        )?;

        let has_secondary_phone = body.secondary_phone.is_some();
        let secondary_phone = match body.secondary_phone {
            Some(sp) => {
                let sanitized = sanitize_string(Some(sp));
                if let Some(ref phone) = sanitized {
                    validate_phone(phone, "Backup phone")?;
                }
                sanitized
            }
            None => existing.secondary_phone,
        };

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
        if has_secondary_phone {
            if let Some(sp) = secondary_phone {
                set_doc.insert("secondary_phone", sp);
            } else {
                set_doc.insert("secondary_phone", mongodb::bson::Bson::Null);
            }
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

/// Refuses to delete (409 `SUPPLIER_HAS_PURCHASES`) while any purchase
/// record still references this supplier by `key` — purchase history is
/// financial/audit data, so it's blocked from ever pointing at nothing
/// (mirrors `inventory::service::category::delete_category`'s
/// `CATEGORY_IN_USE` guard). Once the guard passes, cascades a delete of
/// every `supplier_products` link for this supplier — those are pure
/// linking metadata, not audit history, so no data of lasting value is lost.
pub(crate) async fn delete_supplier(
    db: &Db,
    id: ObjectId,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Supplier> {
    crate::core::logging::domain::tracked("suppliers.deleted", async move {
        let existing = repository::find_supplier_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
            })?;

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This supplier was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": existing.into_supplier(),
                    "conflictingFields": Vec::<String>::new(),
                }),
            ));
        }

        let purchase_count =
            crate::modules::purchases::service::purchase::count_purchases_for_supplier(
                db,
                &existing.key,
            )
            .await?;
        if purchase_count > 0 {
            return Err(AppError::custom(
                StatusCode::CONFLICT,
                codes::SUPPLIER_HAS_PURCHASES,
                format!(
                    "{purchase_count} purchase record(s) still reference supplier '{}'",
                    existing.name
                ),
            ));
        }

        crate::modules::supplier_products::service::link::delete_links_for_supplier(
            db,
            &existing.key,
        )
        .await?;

        let deleted = repository::delete_supplier(db, id, device_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Supplier not found", codes::SUPPLIER_NOT_FOUND)
            })?;

        Ok(deleted.into_supplier())
    })
    .await
}

/// Batch delete for `DELETE /suppliers/batch`. Ids that aren't valid
/// `ObjectId`s, that don't resolve to an existing supplier, or that are
/// blocked by the purchase-history guard are silently skipped rather than
/// failing the whole request — same "valid ones still succeed" semantics as
/// `inventory::service::product::delete_products`.
pub(crate) async fn delete_suppliers(db: &Db, ids: Vec<String>) -> AppResult<u64> {
    crate::core::logging::domain::tracked("suppliers.bulk_deleted", async move {
        let mut deleted_count = 0u64;
        for id in ids {
            if let Ok(object_id) = ObjectId::parse_str(&id)
                && delete_supplier(db, object_id, None, None).await.is_ok()
            {
                deleted_count += 1;
            }
        }
        Ok(deleted_count)
    })
    .await
}

/// Every distinct tag currently in use across all suppliers'
/// `suppliedCategories`, sorted for stable output — backs
/// `GET /suppliers/categories`.
pub(crate) async fn get_supplier_categories(db: &Db) -> AppResult<SupplierCategoriesResponse> {
    let mut categories = repository::distinct_supplied_categories(db).await?;
    categories.sort();
    categories.dedup();
    Ok(SupplierCategoriesResponse { categories })
}

/// Computes the KPI cards for the Suppliers screen.
pub async fn get_supplier_stats(db: &Db) -> AppResult<SupplierStats> {
    let (total_suppliers, direct_contacts_count) = repository::count_stats(db).await?;
    let supply_categories_count = repository::distinct_supplied_categories(db).await?.len() as u64;

    Ok(SupplierStats {
        total_suppliers,
        supply_categories_count,
        direct_contacts_count,
    })
}

/// Converts a page of raw `suppliers` documents — as read by the sync
/// module's cursor scan — into the `Supplier` shape the REST reads return.
/// See `inventory::service::product::hydrate_sync_documents` for why the
/// delta and snapshot feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(documents: Vec<Document>) -> AppResult<Vec<Supplier>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<SupplierDocument>(document)?.into_supplier())
        })
        .collect()
}
