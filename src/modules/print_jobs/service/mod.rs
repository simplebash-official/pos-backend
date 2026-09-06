// Business rules for print-job CRUD/listing — mirrors
// `modules::repairs::service` exactly in shape; differs only in the
// domain-specific validation (job type/quantity vs device model/issue
// description).

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::{build_bson_regex, calculate_pagination, today_utc_range},
    },
    domain::{
        print_jobs::{
            CreatePrintJobRequest, PrintJob, PrintJobAssignment, PrintJobListQuery,
            PrintJobListResponse, PrintJobStats, UpdatePrintJobRequest,
        },
        sequences::ReserveSequenceRequest,
    },
    modules::{
        customers, employees,
        print_jobs::{model::PrintJobDocument, repository},
        sequences,
    },
};

const VALID_STATUSES: &[&str] = &["received", "in_progress", "ready", "delivered", "cancelled"];

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

fn validate_required_fields(
    customer_name: &str,
    job_type: &str,
    quantity: i32,
    estimated_cost_cents: i64,
) -> AppResult<()> {
    if customer_name.trim().is_empty() {
        return Err(AppError::validation("Customer name is required"));
    }
    if job_type.trim().is_empty() {
        return Err(AppError::validation("Job type is required"));
    }
    if quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    if estimated_cost_cents < 0 {
        return Err(AppError::validation("Estimated cost cannot be negative"));
    }
    Ok(())
}

pub async fn list_print_jobs(db: &Db, query: PrintJobListQuery) -> AppResult<PrintJobListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.trim().is_empty()) {
        let pattern = build_bson_regex(search.trim());
        let mut or_clauses = vec![
            doc! { "ticket_number": { "$regex": pattern.clone() } },
            doc! { "customer_name": { "$regex": pattern.clone() } },
            doc! { "customer_phone": { "$regex": pattern.clone() } },
            doc! { "job_type": { "$regex": pattern } },
        ];

        let digits: String = search.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(num) = digits.parse::<u64>() {
            let padded_ticket = format!("PRN-{num:06}");
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
    let print_jobs = documents.into_iter().map(|d| d.into_print_job()).collect();
    let total_pages = total.div_ceil(limit);

    Ok(PrintJobListResponse {
        print_jobs,
        total,
        page,
        limit,
        total_pages,
    })
}

/// Computes the KPI cards for the Print Jobs screen. See `today_utc_range`
/// for how "today" is bounded.
pub async fn get_print_job_stats(db: &Db) -> AppResult<PrintJobStats> {
    let (today_start, today_end) = today_utc_range();
    let agg = repository::aggregate_stats(db, today_start, today_end).await?;

    let avg_job_value_cents = if agg.today_job_count > 0 {
        (agg.today_revenue_cents as f64 / agg.today_job_count as f64).round() as i64
    } else {
        0
    };

    Ok(PrintJobStats {
        today_job_count: agg.today_job_count,
        today_revenue_cents: agg.today_revenue_cents,
        pending_job_count: agg.pending_job_count,
        avg_job_value_cents,
    })
}

pub async fn get_print_job(db: &Db, id_or_key: &str) -> AppResult<PrintJob> {
    let document = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    Ok(document.into_print_job())
}

/// When `customer_key` resolves to a real customer, that record is
/// authoritative for name/phone — a client-typed name/phone alongside a
/// valid key could otherwise silently diverge from the linked customer.
/// Falls back to the client-supplied name/phone only when there's no key
/// (a walk-in with no linked account). Mirrors
/// `billing::service::sale::complete_sale`'s customer-key resolution.
async fn resolve_customer_name_phone(
    db: &Db,
    customer_key: Option<&str>,
    fallback_name: String,
    fallback_phone: Option<String>,
) -> AppResult<(String, Option<String>)> {
    match customer_key {
        Some(key) => {
            let customer = customers::service::get_customer_by_key(db, key).await?;
            Ok((customer.name, Some(customer.primary_phone)))
        }
        None => Ok((fallback_name, fallback_phone)),
    }
}

/// See `repairs::service::resolve_assignment_employee_name` — identical
/// resolve-from-key/server-derives-name/never-trust-client-name rule.
async fn resolve_assignment_employee_name(
    db: &Db,
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

pub async fn create_print_job(
    db: &Db,
    body: CreatePrintJobRequest,
    device_id: Option<String>,
) -> AppResult<PrintJob> {
    let status = body.status.unwrap_or_else(|| "received".to_string());
    validate_status(&status)?;
    validate_promised_ready_at(body.promised_ready_at.as_deref())?;

    let (customer_name, customer_phone) = resolve_customer_name_phone(
        db,
        body.customer.customer_key.as_deref(),
        body.customer.customer_name.unwrap_or_default(),
        body.customer.customer_phone,
    )
    .await?;

    validate_required_fields(
        &customer_name,
        &body.job_type,
        body.quantity,
        body.estimated_cost_cents,
    )?;

    let assignment = body.assignment.unwrap_or_default();
    let PrintJobAssignment {
        assigned_employee_id,
        split_type,
        split_value,
        ..
    } = assignment;
    let assigned_employee_name =
        resolve_assignment_employee_name(db, assigned_employee_id.as_deref()).await?;

    let reservation = sequences::service::reserve_sequence(
        db,
        "printjob".to_string(),
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
    let document = PrintJobDocument {
        id: None,
        key: generate_id(prefixes::PRINT_JOB),
        ticket_number,
        customer_key: body.customer.customer_key,
        customer_name: customer_name.trim().to_string(),
        customer_phone,
        job_type: body.job_type,
        quantity: body.quantity,
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
    Ok(inserted.into_print_job())
}

pub async fn update_print_job(
    db: &Db,
    id_or_key: &str,
    body: UpdatePrintJobRequest,
    device_id: Option<String>,
) -> AppResult<PrintJob> {
    let existing = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    let object_id = existing
        .id
        .expect("persisted print job document must have an _id");

    let customer = body.customer.unwrap_or_default();
    let effective_customer_key = customer
        .customer_key
        .clone()
        .or(existing.customer_key.clone());
    let (customer_name, customer_phone) = resolve_customer_name_phone(
        db,
        effective_customer_key.as_deref(),
        customer.customer_name.unwrap_or(existing.customer_name),
        customer.customer_phone.or(existing.customer_phone),
    )
    .await?;
    let job_type = body.job_type.unwrap_or(existing.job_type);
    let quantity = body.quantity.unwrap_or(existing.quantity);
    let estimated_cost_cents = body
        .estimated_cost_cents
        .unwrap_or(existing.estimated_cost_cents);
    let status = body.status.unwrap_or(existing.status);

    validate_status(&status)?;
    validate_promised_ready_at(body.promised_ready_at.as_deref())?;
    validate_required_fields(&customer_name, &job_type, quantity, estimated_cost_cents)?;

    let mut set_doc = doc! {
        "customer_name": &customer_name,
        "job_type": &job_type,
        "quantity": quantity,
        "estimated_cost_cents": estimated_cost_cents,
        "status": &status,
        "updated_at": BsonDateTime::now(),
    };
    if let Some(key) = effective_customer_key {
        set_doc.insert("customer_key", key);
    }
    match customer_phone {
        Some(phone) => set_doc.insert("customer_phone", phone),
        None => set_doc.insert("customer_phone", mongodb::bson::Bson::Null),
    };
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
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    Ok(updated.into_print_job())
}

pub async fn delete_print_job(
    db: &Db,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<PrintJob> {
    let existing = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    let object_id = existing
        .id
        .expect("persisted print job document must have an _id");

    let deleted = repository::delete(db, object_id, device_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    Ok(deleted.into_print_job())
}

/// Cross-module hook — called by `billing::service::sale::complete_sale`.
/// See `repairs::service::mark_delivered`'s comment for why this is
/// deliberately narrow (status only).
#[allow(dead_code)]
pub(crate) async fn mark_delivered(db: &Db, key: &str) -> AppResult<PrintJob> {
    let updated = repository::set_status_by_key(db, key, "delivered")
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    Ok(updated.into_print_job())
}

/// Converts a page of raw `print_jobs` documents — as read by the sync
/// module's cursor scan — into the `PrintJob` shape the REST reads return.
pub(crate) fn hydrate_sync_documents(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<PrintJob>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<PrintJobDocument>(document)?.into_print_job())
        })
        .collect()
}
