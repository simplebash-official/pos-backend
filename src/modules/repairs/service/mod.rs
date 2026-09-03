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
        utils::{build_bson_regex, calculate_pagination, today_utc_range},
    },
    domain::{
        repairs::{
            CreateRepairRequest, Repair, RepairAssignment, RepairListQuery, RepairListResponse,
            RepairStats, UpdateRepairRequest,
        },
        sequences::ReserveSequenceRequest,
    },
    modules::{
        customers, employees,
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

/// A promised-ready date, when given, must be a real `YYYY-MM-DD` date.
fn validate_promised_ready_at(promised_ready_at: Option<&str>) -> AppResult<()> {
    match promised_ready_at {
        None => Ok(()),
        Some(value) if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() => Ok(()),
        Some(_) => Err(AppError::validation(
            "Promised ready date must be a valid date (YYYY-MM-DD)",
        )),
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
    estimated_cost_cents: Option<i64>,
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
    if let Some(cost) = estimated_cost_cents
        && cost < 0
    {
        return Err(AppError::validation("Estimated cost cannot be negative"));
    }
    Ok(())
}

/// A ticket may sit priceless while `received`, `diagnosing`, `in_repair`
/// or `cancelled`; diagnosis-first repairs often can't be quoted before or
/// during repair work. Once it moves to `ready` or `delivered` a price must
/// exist, since those statuses mean the ticket is headed to pickup/billing.
/// Called on the *effective* post-write status/price so a single
/// PATCH that sets both fields together is validated against the combined
/// result.
const STATUSES_REQUIRING_PRICE: &[&str] = &["ready", "delivered"];

fn validate_price_required_for_status(
    status: &str,
    estimated_cost_cents: Option<i64>,
) -> AppResult<()> {
    if STATUSES_REQUIRING_PRICE.contains(&status) && estimated_cost_cents.is_none() {
        return Err(AppError::validation_with_code(
            format!(
                "A repair price must be set before moving a ticket to '{status}'. Enter the repair price first."
            ),
            codes::REPAIR_PRICE_REQUIRED,
        ));
    }
    Ok(())
}

pub async fn list_repairs(db: &Database, query: RepairListQuery) -> AppResult<RepairListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        let mut or_clauses = vec![
            doc! { "ticket_number": { "$regex": pattern.clone() } },
            doc! { "customer_name": { "$regex": pattern.clone() } },
            doc! { "customer_phone": { "$regex": pattern.clone() } },
            doc! { "device_model": { "$regex": pattern } },
        ];

        let digits: String = search.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(num) = digits.parse::<u64>() {
            let padded_ticket = format!("REP-{num:06}");
            or_clauses.push(doc! { "ticket_number": padded_ticket });
        }

        and_clauses.push(doc! { "$or": or_clauses });
    }
    if let Some(status) = query.status.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "status": status.trim() });
    }
    if query.date_preset.as_deref() == Some("today") {
        let (today_start, today_end) = today_utc_range();
        and_clauses.push(doc! {
            "created_at": {
                "$gte": BsonDateTime::from_chrono(today_start),
                "$lt": BsonDateTime::from_chrono(today_end),
            }
        });
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

/// Computes the KPI cards for the Repair Jobs screen. See `today_utc_range`
/// for how "today" is bounded.
pub async fn get_repair_stats(db: &Database) -> AppResult<RepairStats> {
    let (today_start, today_end) = today_utc_range();
    let agg = repository::aggregate_stats(db, today_start, today_end).await?;

    let avg_job_value_cents = if agg.today_job_count > 0 {
        (agg.today_revenue_cents as f64 / agg.today_job_count as f64).round() as i64
    } else {
        0
    };

    Ok(RepairStats {
        today_job_count: agg.today_job_count,
        today_revenue_cents: agg.today_revenue_cents,
        pending_job_count: agg.pending_job_count,
        avg_job_value_cents,
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

/// When `assigned_employee_id` is present, it's resolved against the
/// `employees` collection and the display name is server-derived from that
/// record — a client-sent `assigned_employee_name` is never trusted once a
/// valid id is given, same "resolved from a key, don't trust a client-sent
/// duplicate" rule `billing::service::sale` applies to a sale item's
/// `productKey`/`assignedEmployeeName`. An unresolvable id 404s
/// (`EMPLOYEE_NOT_FOUND`) before any write. Absent `assigned_employee_id`
/// leaves the ticket unassigned — no employee module lookup happens.
async fn resolve_assignment_employee_name(
    db: &Database,
    assigned_employee_id: Option<&str>,
) -> AppResult<Option<String>> {
    match assigned_employee_id {
        Some(key) => {
            let employee = employees::service::get_employee_by_key(db, key).await?;
            Ok(Some(employee.name))
        }
        None => Ok(None),
    }
}

pub async fn create_repair(
    db: &Database,
    body: CreateRepairRequest,
    device_id: Option<String>,
) -> AppResult<Repair> {
    let status = body.status.unwrap_or_else(|| "received".to_string());
    validate_status(&status)?;
    validate_promised_ready_at(body.promised_ready_at.as_deref())?;

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
    validate_price_required_for_status(&status, body.estimated_cost_cents)?;

    let assignment = body.assignment.unwrap_or_default();
    let RepairAssignment {
        assigned_employee_id,
        split_type,
        split_value,
        ..
    } = assignment;
    let assigned_employee_name =
        resolve_assignment_employee_name(db, assigned_employee_id.as_deref()).await?;

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
        promised_ready_at: body.promised_ready_at,
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
    let estimated_cost_cents = body.estimated_cost_cents.or(existing.estimated_cost_cents);
    let status = body.status.unwrap_or(existing.status);

    validate_status(&status)?;
    validate_promised_ready_at(body.promised_ready_at.as_deref())?;
    validate_required_fields(
        &customer_name,
        &customer_phone,
        &device_model,
        &issue_description,
        estimated_cost_cents,
    )?;
    validate_price_required_for_status(&status, estimated_cost_cents)?;

    let mut set_doc = doc! {
        "customer_name": &customer_name,
        "customer_phone": &customer_phone,
        "device_model": &device_model,
        "issue_description": &issue_description,
        "status": &status,
        "updated_at": BsonDateTime::now(),
    };
    match estimated_cost_cents {
        Some(cost) => {
            set_doc.insert("estimated_cost_cents", cost);
        }
        None => {
            set_doc.insert("estimated_cost_cents", mongodb::bson::Bson::Null);
        }
    }
    if let Some(key) = effective_customer_key {
        set_doc.insert("customer_key", key);
    }
    if let Some(sn) = body.serial_number {
        set_doc.insert("serial_number", sn);
    }
    if let Some(promised) = body.promised_ready_at {
        set_doc.insert("promised_ready_at", promised);
    }
    if let Some(mc) = body.material_cost_cents {
        set_doc.insert("material_cost_cents", mc);
    }
    if let Some(assignment) = body.assignment {
        if let Some(eid) = assignment.assigned_employee_id {
            let ename = resolve_assignment_employee_name(db, Some(&eid)).await?;
            set_doc.insert("assigned_employee_id", eid);
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
