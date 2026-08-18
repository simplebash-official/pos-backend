// Business rules for repair-ticket CRUD/listing: required-field validation,
// status validation, ticket-number reservation on create, and the narrow
// `mark_delivered` cross-module hook. Delegates all Mongo access to
// `super::repository`.

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
    domain::{
        repairs::{
            CreateRepairRequest, Repair, RepairAssignment, RepairListQuery, RepairListResponse,
            UpdateRepairRequest,
        },
        sequences::ReserveSequenceRequest,
    },
    modules::{
        customers,
        repairs::{model::RepairDocument, repository},
        sequences,
    },
};

const VALID_STATUSES: &[&str] = &[
    "received",
    "diagnosing",
    "in_repair",
    "ready",
    "delivered",
    "cancelled",
];

fn validate_status(status: &str) -> AppResult<()> {
    if VALID_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(AppError::validation_with_code(
            format!(
                "Invalid status '{status}'. Must be one of: {}",
                VALID_STATUSES.join(", ")
            ),
            codes::INVALID_JOB_STATUS,
        ))
    }
}

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

fn validate_required_fields(
    customer_name: &str,
    customer_phone: &str,
    device_model: &str,
    issue_description: &str,
    estimated_cost_cents: i64,
) -> AppResult<()> {
    if customer_name.trim().is_empty() {
        return Err(AppError::validation("Customer name is required"));
    }
    validate_phone_number(customer_phone, "Customer phone")?;
    if device_model.trim().is_empty() {
        return Err(AppError::validation("Device model is required"));
    }
    if issue_description.trim().is_empty() {
        return Err(AppError::validation("Issue description is required"));
    }
    if estimated_cost_cents < 0 {
        return Err(AppError::validation("Estimated cost cannot be negative"));
    }
    Ok(())
}

pub async fn list_repairs(db: &Database, query: RepairListQuery) -> AppResult<RepairListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        and_clauses.push(doc! {
            "$or": [
                { "ticket_number": { "$regex": pattern.clone() } },
                { "customer_name": { "$regex": pattern.clone() } },
                { "customer_phone": { "$regex": pattern.clone() } },
                { "device_model": { "$regex": pattern } },
            ]
        });
    }
    if let Some(status) = query.status.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "status": status.trim() });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);
    let (documents, total) = repository::list(db, filter, skip, limit).await?;
    let repairs = documents.into_iter().map(|d| d.into_repair()).collect();
    let total_pages = total.div_ceil(limit);

    Ok(RepairListResponse {
        repairs,
        total,
        page,
        limit,
        total_pages,
    })
}

pub async fn get_repair(db: &Database, id_or_key: &str) -> AppResult<Repair> {
    let document = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    Ok(document.into_repair())
}

/// When `customer_key` resolves to a real customer, that record is
/// authoritative for name/phone — a client-typed name/phone alongside a
/// valid key could otherwise silently diverge from the linked customer.
/// Falls back to the client-supplied name/phone only when there's no key
/// (a walk-in with no linked account). Mirrors
/// `billing::service::sale::complete_sale`'s customer-key resolution.
async fn resolve_customer_name_phone(
    db: &Database,
    customer_key: Option<&str>,
    fallback_name: String,
    fallback_phone: String,
) -> AppResult<(String, String)> {
    match customer_key {
        Some(key) => {
            let customer = customers::service::get_customer_by_key(db, key).await?;
            Ok((customer.name, customer.primary_phone))
        }
        None => Ok((fallback_name, fallback_phone)),
    }
}

pub async fn create_repair(
    db: &Database,
    body: CreateRepairRequest,
    device_id: Option<String>,
) -> AppResult<Repair> {
    let status = body.status.unwrap_or_else(|| "received".to_string());
    validate_status(&status)?;

    let (customer_name, customer_phone) = resolve_customer_name_phone(
        db,
        body.customer.customer_key.as_deref(),
        body.customer.customer_name.unwrap_or_default(),
        body.customer.customer_phone.unwrap_or_default(),
    )
    .await?;

    validate_required_fields(
        &customer_name,
        &customer_phone,
        &body.device_model,
        &body.issue_description,
        body.estimated_cost_cents,
    )?;

    let assignment = body.assignment.unwrap_or_default();
    let RepairAssignment {
        assigned_employee_id,
        assigned_employee_name,
        split_type,
        split_value,
    } = assignment;

    let reservation = sequences::service::reserve_sequence(
        db,
        "repair".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id: device_id.clone(),
        },
    )
    .await?;
    let ticket_number = format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    );

    let now = BsonDateTime::now();
    let document = RepairDocument {
        id: None,
        key: generate_id(prefixes::REPAIR),
        ticket_number,
        customer_key: body.customer.customer_key,
        customer_name: customer_name.trim().to_string(),
        customer_phone: customer_phone.trim().to_string(),
        device_model: body.device_model,
        serial_number: body.serial_number,
        issue_description: body.issue_description,
        status,
        estimated_cost_cents: body.estimated_cost_cents,
        material_cost_cents: body.material_cost_cents,
        assigned_employee_id,
        assigned_employee_name,
        split_type,
        split_value,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        updated_by_device: device_id,
    };

    let inserted = repository::insert(db, document).await?;
    Ok(inserted.into_repair())
}

pub async fn update_repair(
    db: &Database,
    id_or_key: &str,
    body: UpdateRepairRequest,
    device_id: Option<String>,
) -> AppResult<Repair> {
    let existing = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    let object_id = existing
        .id
        .expect("persisted repair document must have an _id");

    let customer = body.customer.unwrap_or_default();
    let effective_customer_key = customer
        .customer_key
        .clone()
        .or(existing.customer_key.clone());
    let (customer_name, customer_phone) = resolve_customer_name_phone(
        db,
        effective_customer_key.as_deref(),
        customer.customer_name.unwrap_or(existing.customer_name),
        customer.customer_phone.unwrap_or(existing.customer_phone),
    )
    .await?;
    let device_model = body.device_model.unwrap_or(existing.device_model);
    let issue_description = body.issue_description.unwrap_or(existing.issue_description);
    let estimated_cost_cents = body
        .estimated_cost_cents
        .unwrap_or(existing.estimated_cost_cents);
    let status = body.status.unwrap_or(existing.status);

    validate_status(&status)?;
    validate_required_fields(
        &customer_name,
        &customer_phone,
        &device_model,
        &issue_description,
        estimated_cost_cents,
    )?;

    let mut set_doc = doc! {
        "customer_name": &customer_name,
        "customer_phone": &customer_phone,
        "device_model": &device_model,
        "issue_description": &issue_description,
        "estimated_cost_cents": estimated_cost_cents,
        "status": &status,
        "updated_at": BsonDateTime::now(),
    };
    if let Some(key) = effective_customer_key {
        set_doc.insert("customer_key", key);
    }
    if let Some(sn) = body.serial_number {
        set_doc.insert("serial_number", sn);
    }
    if let Some(mc) = body.material_cost_cents {
        set_doc.insert("material_cost_cents", mc);
    }
    if let Some(assignment) = body.assignment {
        if let Some(eid) = assignment.assigned_employee_id {
            set_doc.insert("assigned_employee_id", eid);
        }
        if let Some(ename) = assignment.assigned_employee_name {
            set_doc.insert("assigned_employee_name", ename);
        }
        if let Some(st) = assignment.split_type {
            set_doc.insert("split_type", st);
        }
        if let Some(sv) = assignment.split_value {
            set_doc.insert("split_value", sv);
        }
    }
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }

    let updated = repository::update(db, object_id, set_doc)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    Ok(updated.into_repair())
}

pub async fn delete_repair(
    db: &Database,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<Repair> {
    let existing = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    let object_id = existing
        .id
        .expect("persisted repair document must have an _id");

    let deleted = repository::delete(db, object_id, device_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    Ok(deleted.into_repair())
}

/// Cross-module hook — called by `billing::service::sale::complete_sale`
/// when a cart line bills a repair ticket. Narrow on purpose: only flips
/// `status`, never touches cost/assignment fields, since a sale completing
/// shouldn't silently rewrite ticket details a technician entered.
#[allow(dead_code)]
pub(crate) async fn mark_delivered(db: &Database, key: &str) -> AppResult<Repair> {
    let updated = repository::set_status_by_key(db, key, "delivered")
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    Ok(updated.into_repair())
}

/// Converts a page of raw `repairs` documents — as read by the sync
/// module's cursor scan — into the `Repair` shape the REST reads return.
/// See `suppliers::service::hydrate_sync_documents` for why the delta and
/// snapshot feeds must produce identical rows.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<Repair>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<RepairDocument>(document)?.into_repair())
        })
        .collect()
}
