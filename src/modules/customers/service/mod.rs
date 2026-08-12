// Business rules for customer CRUD/listing: required-field validation,
// quick-create support, phone/email format checks, search/filtering,
// and the financial debt guard that prevents deleting a customer with
// an outstanding balance. Delegates all Mongo access to `super::repository`.

use axum::http::StatusCode;
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::{build_bson_regex, calculate_pagination},
    },
    domain::customers::{
        CreateCustomerRequest, Customer, CustomerListQuery, CustomerListResponse,
        CustomerTagsResponse, UpdateCustomerRequest,
    },
    modules::customers::{model::CustomerDocument, repository},
};

fn validate_phone_number(phone: &str, field_name: &str) -> AppResult<()> {
    let trimmed = phone.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(format!(
            "{field_name} cannot be empty"
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(format!(
            "{field_name} may only contain digits, spaces, dashes, and a plus sign"
        )));
    }
    Ok(())
}

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

fn sanitize_string(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn sanitize_tags(tags: Vec<String>) -> Vec<String> {
    let mut cleaned: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    cleaned.sort();
    cleaned.dedup();
    cleaned
}

fn sort_field_for(sort_by: Option<&str>) -> &'static str {
    match sort_by {
        Some("name") => "name",
        Some("primaryPhone") => "primary_phone",
        Some("outstandingBalanceCents") => "outstanding_balance_cents",
        Some("totalPurchasesCents") => "total_purchases_cents",
        Some("createdAt") => "created_at",
        Some("updatedAt") => "updated_at",
        _ => "updated_at",
    }
}

pub async fn list_customers(
    db: &Database,
    query: CustomerListQuery,
) -> AppResult<CustomerListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "primary_phone": { "$regex": pattern.clone() } },
                { "secondary_phone": { "$regex": pattern.clone() } },
                { "email": { "$regex": pattern.clone() } },
                { "address": { "$regex": pattern.clone() } },
                { "contact_person": { "$regex": pattern } },
            ]
        });
    }

    if let Some(tag) = query.tag.filter(|t| !t.trim().is_empty()) {
        and_clauses.push(doc! { "tags": tag.trim() });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let sort_field = sort_field_for(query.sort_by.as_deref());
    let sort_order = if query.sort_order.as_deref() == Some("asc")
        || (query.sort_by.as_deref() == Some("name") && query.sort_order.is_none())
    {
        1
    } else {
        -1
    };
    let sort = doc! { sort_field: sort_order };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 10, 100);

    let (documents, total) = repository::list_customers(db, filter, sort, skip, limit).await?;
    let customers = documents.into_iter().map(|d| d.into_customer()).collect();
    let total_pages = total.div_ceil(limit);

    Ok(CustomerListResponse {
        customers,
        total,
        page,
        limit,
        total_pages,
    })
}

pub async fn get_customer(db: &Database, id_or_key: &str) -> AppResult<Customer> {
    let document = repository::find_customer_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
        })?;

    Ok(document.into_customer())
}

pub async fn create_customer(
    db: &Database,
    body: CreateCustomerRequest,
    device_id: Option<String>,
) -> AppResult<Customer> {
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
}

pub async fn replace_customer(
    db: &Database,
    id_or_key: &str,
    body: CreateCustomerRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Customer> {
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
}

pub async fn update_customer(
    db: &Database,
    id_or_key: &str,
    body: UpdateCustomerRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Customer> {
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
}

pub async fn delete_customer(
    db: &Database,
    id_or_key: &str,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Customer> {
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

    if existing.outstanding_balance_cents != 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::CUSTOMER_HAS_OUTSTANDING_BALANCE,
            format!(
                "Cannot delete customer '{}' with an outstanding balance of {} cents",
                existing.name, existing.outstanding_balance_cents
            ),
        ));
    }

    let deleted = repository::delete_customer(db, object_id, device_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
        })?;

    Ok(deleted.into_customer())
}

pub async fn delete_customers(db: &Database, ids: Vec<String>) -> AppResult<u64> {
    let mut deleted_count = 0u64;
    for id in ids {
        if delete_customer(db, &id, None, None).await.is_ok() {
            deleted_count += 1;
        }
    }
    Ok(deleted_count)
}

pub async fn get_customer_tags(db: &Database) -> AppResult<CustomerTagsResponse> {
    let mut tags = repository::distinct_tags(db).await?;
    tags.sort();
    tags.dedup();
    Ok(CustomerTagsResponse { tags })
}

/// Cross-module customer lookup by key — called by billing/invoices/repairs modules.
#[allow(dead_code)]
pub(crate) async fn get_customer_by_key(db: &Database, key: &str) -> AppResult<Customer> {
    let doc = repository::find_customer_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
        })?;
    Ok(doc.into_customer())
}

/// Cross-module customer financial adjustment — called by billing/invoices/payments modules.
#[allow(dead_code)]
pub(crate) async fn apply_financial_delta(
    db: &Database,
    key: &str,
    purchases_delta: i64,
    balance_delta: i64,
) -> AppResult<()> {
    repository::adjust_customer_financials(db, key, purchases_delta, balance_delta)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Customer not found", codes::CUSTOMER_NOT_FOUND)
        })?;
    Ok(())
}
