use super::print_job_alerts;
use super::print_job_items::{price_items, use_parts};
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::utils::{build_bson_regex, calculate_pagination, shop_today_range};
use crate::domain::billing::Handover;
use crate::domain::customers::Customer;
use crate::domain::inventory::StockMovementType;
use crate::domain::print_jobs::{
    CreatePrintJobRequest, PrintItemSource, PrintJob, PrintJobAssignment, PrintJobItem,
    PrintJobKind, PrintJobListQuery, PrintJobListResponse, PrintJobStats, UpdatePrintJobRequest,
};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::modules::print_jobs::model::PrintJobDocument;
use crate::modules::print_jobs::repository;
use crate::modules::warranties::service::WarrantyActor;
use crate::modules::{billing, customers, employees, sequences, warranties};
use chrono::Utc;
use mongodb::bson::{Bson, DateTime as BsonDateTime, Document, doc};

/// The most characters a staff note can have.
const NOTE_MAX: usize = 1000;

/// Creates the print job.
pub async fn create_print_job(
    db: &Db,
    body: CreatePrintJobRequest,
    device_id: Option<String>,
) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.created", async move {
        let status = body.status.unwrap_or_else(|| "received".to_string());
        validate_status(&status)?;
        validate_job_type(&body.job_type)?;
        validate_promised_ready_at(body.promised_ready_at.as_deref())?;
        let note = match &body.note {
            Some(typed) => clean_note(typed)?,
            None => None,
        };

        let customer =
            customers::service::get_customer_by_key(db, &body.customer.customer_key).await?;

        validate_required_fields(&body.job_type, body.quantity)?;
        let kind = PrintJobKind::Standard;
        let priced = price_items(db, body.items, &[], &kind).await?;
        validate_price_required_for_status(&status, &kind, &priced.items)?;
        if status == "delivered" {
            refuse_handing_back_unbilled_parts(&kind, &priced.items, None)?;
        }

        let assignment = body.assignment.unwrap_or_default();
        let PrintJobAssignment {
            assigned_employee_id,
            split_type,
            split_value,
            ..
        } = assignment;
        let assigned_employee_name = match assigned_employee_id.as_deref() {
            Some(key) => Some(
                employees::service::resolve_assignment(db, key, split_type.as_deref(), split_value)
                    .await?
                    .name,
            ),
            None => None,
        };

        insert_job(
            db,
            NewJob {
                customer: customer.into(),
                job_type: body.job_type,
                quantity: body.quantity,
                promised_ready_at: body.promised_ready_at,
                status,
                note,
                kind,
                items: priced.items,
                price_cents: priced.price_cents,
                cost_cents: priced.cost_cents,
                assigned_employee_id,
                assigned_employee_name,
                split_type,
                split_value,
                device_id,
            },
        )
        .await
    })
    .await
}

/// Whom a job is for, as it is copied onto the job.
#[derive(Debug)]
struct JobCustomer {
    key: String,
    name: String,
    phone: String,
}

impl From<Customer> for JobCustomer {
    fn from(customer: Customer) -> Self {
        JobCustomer {
            key: customer.key,
            name: customer.name,
            phone: customer.primary_phone,
        }
    }
}

/// A job whose every field is already checked, ready to be numbered and saved.
#[derive(Debug)]
struct NewJob {
    customer: JobCustomer,
    job_type: String,
    quantity: i32,
    promised_ready_at: Option<String>,
    status: String,
    note: Option<String>,
    kind: PrintJobKind,
    items: Vec<PrintJobItem>,
    price_cents: i64,
    cost_cents: i64,
    assigned_employee_id: Option<String>,
    assigned_employee_name: Option<String>,
    split_type: Option<String>,
    split_value: Option<f64>,
    device_id: Option<String>,
}

/// Reserves the next `PRN-000001` number and saves the job under it.
async fn insert_job(db: &Db, job: NewJob) -> AppResult<PrintJob> {
    let reservation = sequences::service::reserve_sequence(
        db,
        "printjob".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id: job.device_id.clone(),
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
        customer_key: job.customer.key,
        customer_name: job.customer.name,
        customer_phone: job.customer.phone,
        job_type: job.job_type,
        quantity: job.quantity,
        promised_ready_at: job.promised_ready_at,
        status: job.status,
        note: job.note,
        kind: job.kind,
        items: job.items,
        price_cents: job.price_cents,
        cost_cents: job.cost_cents,
        assigned_employee_id: job.assigned_employee_id,
        assigned_employee_name: job.assigned_employee_name,
        split_type: job.split_type,
        split_value: job.split_value,
        billed_invoice_key: None,
        billed_invoice_number: None,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        updated_by_device: job.device_id,
    };

    Ok(repository::insert(db, document).await?.into_print_job())
}

/// What a warranty claim on print work needs to open its rework job.
#[derive(Debug)]
pub(crate) struct NewPrintRework<'a> {
    pub claim_key: &'a str,
    pub claim_number: &'a str,
    pub original_print_job_key: &'a str,
    /// What the customer says is wrong now.
    pub issue_description: &'a str,
    pub assigned_employee_key: Option<&'a str>,
    pub device_id: Option<String>,
}

/// The job a warranty came from, even when it has since been deleted from the list.
async fn require_original(db: &Db, key: &str) -> AppResult<PrintJobDocument> {
    repository::find_by_keys(db, &[key.to_string()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })
}

/// Opens a free warranty rework of an earlier job: same customer, type and order quantity, the
/// customer's complaint as its note, no lines yet (the person adds the work and any parts used),
/// and a `WarrantyRework` kind that links back to the claim and the original job.
pub(crate) async fn create_rework(db: &Db, rework: NewPrintRework<'_>) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.rework_created", async move {
        let original = require_original(db, rework.original_print_job_key).await?;
        let assigned_employee_name = match rework.assigned_employee_key {
            Some(key) => Some(
                employees::service::resolve_assignment(db, key, None, None)
                    .await?
                    .name,
            ),
            None => None,
        };
        insert_job(
            db,
            NewJob {
                customer: JobCustomer {
                    key: original.customer_key,
                    name: original.customer_name,
                    phone: original.customer_phone,
                },
                job_type: original.job_type,
                quantity: original.quantity,
                promised_ready_at: None,
                status: "received".to_string(),
                note: Some(rework.issue_description.to_string()),
                kind: PrintJobKind::WarrantyRework {
                    claim_key: rework.claim_key.to_string(),
                    claim_number: rework.claim_number.to_string(),
                    original_print_job_key: original.key,
                    original_ticket_number: original.ticket_number,
                },
                items: Vec::new(),
                price_cents: 0,
                cost_cents: 0,
                assigned_employee_id: rework.assigned_employee_key.map(str::to_string),
                assigned_employee_name,
                split_type: None,
                split_value: None,
                device_id: rework.device_id,
            },
        )
        .await
    })
    .await
}

/// What a rejected warranty claim on print work needs to open a paid job for the same work.
#[derive(Debug)]
pub(crate) struct NewPaidJob<'a> {
    pub customer_key: &'a str,
    pub original_print_job_key: &'a str,
    /// What the customer says is wrong, and why the claim was not covered.
    pub note: String,
    pub device_id: Option<String>,
}

/// Opens an ordinary, still unpriced print job of the same type and order quantity as the one a
/// rejected claim was about.
pub(crate) async fn create_paid_job(db: &Db, paid: NewPaidJob<'_>) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.created", async move {
        let original = require_original(db, paid.original_print_job_key).await?;
        let customer = customers::service::get_customer_by_key(db, paid.customer_key).await?;
        insert_job(
            db,
            NewJob {
                customer: customer.into(),
                job_type: original.job_type,
                quantity: original.quantity,
                promised_ready_at: None,
                status: "received".to_string(),
                note: Some(paid.note),
                kind: PrintJobKind::Standard,
                items: Vec::new(),
                price_cents: 0,
                cost_cents: 0,
                assigned_employee_id: None,
                assigned_employee_name: None,
                split_type: None,
                split_value: None,
                device_id: paid.device_id,
            },
        )
        .await
    })
    .await
}

/// Deletes the print job.
pub async fn delete_print_job(
    db: &Db,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.deleted", async move {
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
    })
    .await
}

/// Returns the print job.
pub async fn get_print_job(db: &Db, id_or_key: &str) -> AppResult<PrintJob> {
    let document = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
        })?;
    Ok(document.into_print_job())
}

/// Computes the KPI cards for the Print Jobs screen. See `shop_today_range`
/// for how "today" is bounded.
pub async fn get_print_job_stats(db: &Db) -> AppResult<PrintJobStats> {
    let (today_start, today_end) = shop_today_range();
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

/// Lists the print jobs.
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
    if let Some(customer_key) = query.customer_key {
        if customer_key.trim().is_empty() {
            return Err(AppError::validation("customerKey cannot be blank."));
        }
        and_clauses.push(doc! { "customer_key": customer_key.trim() });
    }
    if let Some(status) = query.status.filter(|s| !s.trim().is_empty()) {
        and_clauses.push(doc! { "status": status.trim() });
    }
    if query.date_preset.as_deref() == Some("today") {
        let (today_start, today_end) = shop_today_range();
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

/// Cross-module hook, called by `billing::service::sale::complete_sale` when an invoice bills a
/// print job. Same contract as `repairs::service::mark_billed`: stamps the job with the invoice
/// and, for `Handover::Now`, sets `status` to `delivered`; a job already billed is a 409
/// (`TICKET_ALREADY_BILLED`).
pub(crate) async fn mark_billed(
    db: &Db,
    key: &str,
    invoice_key: &str,
    invoice_number: &str,
    handover: Handover,
) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.billed", async move {
        let status = match handover {
            Handover::Now => Some("delivered"),
            Handover::Later => None,
        };
        if let Some(updated) =
            repository::stamp_billed(db, key, invoice_key, invoice_number, status).await?
        {
            let job = updated.into_print_job();
            if handover == Handover::Now {
                // The status before the write is gone, so only "delivered" is compared: a job
                // billed twice raises one notice (the dedupe key holds the status).
                print_job_alerts::announce_status_change(db, &job, "").await?;
            }
            return Ok(job);
        }
        let existing = repository::find_by_id_or_key(db, key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
            })?;
        Err(AppError::conflict(
            codes::TICKET_ALREADY_BILLED,
            format!(
                "Print job {} was already billed on {}",
                existing.ticket_number,
                existing
                    .billed_invoice_number
                    .as_deref()
                    .expect("a job that refused to be billed again carries its billing mark"),
            ),
        ))
    })
    .await
}

/// Cross-module hook, called by `billing::service::sale::void_invoice`: removes the billing mark
/// the voided invoice left on a print job. Returns whether the job still carried that mark.
pub(crate) async fn clear_billed(db: &Db, key: &str, invoice_key: &str) -> AppResult<bool> {
    crate::core::logging::domain::tracked("print_jobs.unbilled", async move {
        repository::clear_billed_stamp(db, key, invoice_key).await
    })
    .await
}

/// Updates the print job.
pub(crate) async fn update_print_job(
    db: &Db,
    id_or_key: &str,
    body: UpdatePrintJobRequest,
    device_id: Option<String>,
    actor: &WarrantyActor,
) -> AppResult<PrintJob> {
    crate::core::logging::domain::tracked("print_jobs.updated", async move {
        let existing = repository::find_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
            })?;
        let object_id = existing
            .id
            .expect("persisted print job document must have an _id");

        let customer_key = match &body.customer {
            Some(customer) => customer.customer_key.as_str(),
            None => existing.customer_key.as_str(),
        };
        let customer = customers::service::get_customer_by_key(db, customer_key).await?;
        let job_type = body.job_type.unwrap_or(existing.job_type);
        let quantity = body.quantity.unwrap_or(existing.quantity);
        let was_delivered = existing.status == "delivered";
        let previous_status = existing.status.clone();
        let status = body.status.unwrap_or(existing.status);
        let billed_invoice_key = existing.billed_invoice_key.as_deref();

        validate_status(&status)?;
        validate_job_type(&job_type)?;
        validate_promised_ready_at(body.promised_ready_at.as_deref())?;
        validate_required_fields(&job_type, quantity)?;
        let priced = match body.items {
            Some(_) if billed_invoice_key.is_some() => {
                return Err(AppError::conflict(
                    codes::PRINT_JOB_ITEMS_LOCKED,
                    "This job has been billed; its items can no longer be changed. Void the \
                     invoice to change them.",
                ));
            }
            Some(_) if was_delivered => {
                return Err(AppError::conflict(
                    codes::PRINT_JOB_ITEMS_LOCKED,
                    "This job has been handed back; its items can no longer be changed.",
                ));
            }
            Some(inputs) => Some(price_items(db, inputs, &existing.items, &existing.kind).await?),
            None => None,
        };
        let items = match &priced {
            Some(priced) => &priced.items,
            None => &existing.items,
        };
        validate_price_required_for_status(&status, &existing.kind, items)?;
        let handing_back = status == "delivered" && !was_delivered;
        if handing_back {
            refuse_handing_back_unbilled_parts(&existing.kind, items, billed_invoice_key)?;
        }

        let mut set_doc = doc! {
            "customer_key": &customer.key,
            "customer_name": &customer.name,
            "customer_phone": &customer.primary_phone,
            "job_type": &job_type,
            "quantity": quantity,
            "status": &status,
            "updated_at": BsonDateTime::now(),
        };
        if let Some(priced) = &priced {
            set_doc.insert("items", mongodb::bson::serialize_to_bson(&priced.items)?);
            set_doc.insert("price_cents", priced.price_cents);
            set_doc.insert("cost_cents", priced.cost_cents);
        }
        if let Some(typed) = &body.note {
            match clean_note(typed)? {
                Some(note) => set_doc.insert("note", note),
                None => set_doc.insert("note", Bson::Null),
            };
        }
        if let Some(promised) = body.promised_ready_at {
            set_doc.insert("promised_ready_at", promised);
        }
        if let Some(assignment) = body.assignment {
            // A split sent with no employee still has to be checked against who the job is
            // already assigned to.
            let employee_key = assignment
                .assigned_employee_id
                .as_deref()
                .or(existing.assigned_employee_id.as_deref());
            if let Some(key) = employee_key {
                let assignee = employees::service::resolve_assignment(
                    db,
                    key,
                    assignment.split_type.as_deref(),
                    assignment.split_value,
                )
                .await?;
                if let Some(eid) = &assignment.assigned_employee_id {
                    set_doc.insert("assigned_employee_id", eid);
                    set_doc.insert("assigned_employee_name", assignee.name);
                    if assignee.is_owner {
                        // Moving a job to the owner drops any commission it carried.
                        set_doc.insert("split_type", Bson::Null);
                        set_doc.insert("split_value", Bson::Null);
                    }
                }
            }
            if let Some(st) = assignment.split_type {
                set_doc.insert("split_type", st);
            }
            if let Some(sv) = assignment.split_value {
                set_doc.insert("split_value", sv);
            }
        }
        if let (true, Some(invoice_key)) = (handing_back, billed_invoice_key) {
            // Billed with the hand-over later: the warranties start today, before the status
            // moves, so a failure here leaves the job open and the hand-back can be retried.
            issue_handback_warranties(
                db,
                HandedBack {
                    // The customer the job names after this change, as the write below stores.
                    customer: warranties::service::JobCustomer {
                        key: &customer.key,
                        name: &customer.name,
                        phone: &customer.primary_phone,
                    },
                    print_job_key: &existing.key,
                    ticket_number: &existing.ticket_number,
                    items,
                },
                invoice_key,
                actor,
                device_id.clone(),
            )
            .await?;
        }
        if let Some(device) = &device_id {
            set_doc.insert("updated_by_device", device.clone());
        }

        let updated = repository::update(db, object_id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Print job not found", codes::PRINT_JOB_NOT_FOUND)
            })?
            .into_print_job();
        print_job_alerts::announce_status_change(db, &updated, &previous_status).await?;
        if handing_back && updated.kind.is_rework() {
            // A rework is never billed, so its parts leave stock here, against the job itself.
            use_parts(
                db,
                &updated,
                StockMovementType::WarrantyReworkPartConsumption,
                &updated.key,
            )
            .await?;
        }
        Ok(updated)
    })
    .await
}

/// A job's stock parts leave stock with its invoice, so a job that has parts from stock is handed
/// back only once it is billed (or by billing it with the hand-over now). Marking an unbilled job
/// delivered by hand would leave its parts in stock forever.
fn refuse_handing_back_unbilled_parts(
    kind: &PrintJobKind,
    items: &[PrintJobItem],
    billed_invoice_key: Option<&str>,
) -> AppResult<()> {
    let has_stock_part = items
        .iter()
        .any(|item| matches!(item.source, PrintItemSource::Stock { .. }));
    if !kind.is_rework() && has_stock_part && billed_invoice_key.is_none() {
        return Err(AppError::validation_with_code(
            "Bill this job to hand it back: its parts from stock leave stock with the invoice.",
            codes::PRINT_JOB_PART_INVALID,
        ));
    }
    Ok(())
}

/// The job being handed back, as its warranties need it.
#[derive(Debug)]
struct HandedBack<'a> {
    customer: warranties::service::JobCustomer<'a>,
    print_job_key: &'a str,
    ticket_number: &'a str,
    items: &'a [PrintJobItem],
}

/// Gives the lines of a job billed with the hand-over later their warranties, starting today, for
/// the job's own customer, on the invoice that billed it (its number and line). Does nothing when
/// that invoice already gave the job warranties: a job reopened and handed back again keeps its
/// first ones. A rework is never billed, so it never gets here.
async fn issue_handback_warranties(
    db: &Db,
    job: HandedBack<'_>,
    invoice_key: &str,
    actor: &WarrantyActor,
    device_id: Option<String>,
) -> AppResult<()> {
    if warranties::service::exist_for_print_job_on_invoice(db, job.print_job_key, invoice_key)
        .await?
    {
        return Ok(());
    }
    let invoice = billing::service::get_invoice(db, invoice_key).await?;
    let line_index = invoice
        .items
        .iter()
        .position(|line| line.source_ticket_key.as_deref() == Some(job.print_job_key))
        .ok_or_else(|| {
            AppError::internal(format!(
                "invoice {} does not bill print job {}, which names it as its invoice",
                invoice.invoice_number, job.ticket_number
            ))
        })?;
    warranties::service::issue_for_print_job(
        db,
        warranties::service::PrintJobLine {
            invoice_key: &invoice.key,
            invoice_number: &invoice.invoice_number,
            delivered_at: Utc::now(),
            customer: job.customer,
            line_index,
            print_job_key: job.print_job_key,
            ticket_number: job.ticket_number,
            items: job.items,
            actor,
            device_id,
        },
    )
    .await?;
    Ok(())
}

/// A job may sit with no lines while `received`, `in_repair` (in production) or `cancelled`.
/// Once it moves to `ready` or `delivered` it needs at least one line, since those statuses mean
/// it is headed to pickup and billing. A warranty rework charges nothing, so it needs none.
/// Called on the *effective* post-write status and lines, so one change that sets both together
/// is checked against the combined result.
const STATUSES_REQUIRING_PRICE: &[&str] = &["ready", "delivered"];

fn validate_price_required_for_status(
    status: &str,
    kind: &PrintJobKind,
    items: &[PrintJobItem],
) -> AppResult<()> {
    if STATUSES_REQUIRING_PRICE.contains(&status) && !kind.is_rework() && items.is_empty() {
        return Err(AppError::validation_with_code(
            format!(
                "Add the items before moving a print job to '{status}'. A job needs at least one item."
            ),
            codes::PRINT_JOB_PRICE_REQUIRED,
        ));
    }
    Ok(())
}

/// A staff note as typed: trimmed, and `None` when it is blank.
fn clean_note(typed: &str) -> AppResult<Option<String>> {
    let note = typed.trim();
    if note.chars().count() > NOTE_MAX {
        return Err(AppError::validation(format!(
            "A note can be at most {NOTE_MAX} characters."
        )));
    }
    Ok((!note.is_empty()).then(|| note.to_string()))
}

const VALID_JOB_TYPES: &[&str] = &[
    "mug",
    "t-shirt",
    "handbill",
    "banner",
    "photo-framing",
    "custom",
];

pub(super) fn validate_job_type(job_type: &str) -> AppResult<()> {
    if VALID_JOB_TYPES.contains(&job_type) {
        Ok(())
    } else {
        Err(AppError::validation_with_code(
            format!(
                "Invalid job type '{job_type}'. Must be one of: {}",
                VALID_JOB_TYPES.join(", ")
            ),
            codes::INVALID_JOB_TYPE,
        ))
    }
}

/// A promised-ready date, when given, must be a real `YYYY-MM-DD` date.
pub(super) fn validate_promised_ready_at(promised_ready_at: Option<&str>) -> AppResult<()> {
    match promised_ready_at {
        None => Ok(()),
        Some(value) if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() => Ok(()),
        Some(_) => Err(AppError::validation(
            "Promised ready date must be a valid date (YYYY-MM-DD)",
        )),
    }
}

pub(super) fn validate_required_fields(job_type: &str, quantity: i32) -> AppResult<()> {
    if job_type.trim().is_empty() {
        return Err(AppError::validation("Job type is required"));
    }
    if quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    Ok(())
}

const VALID_STATUSES: &[&str] = &["received", "in_repair", "ready", "delivered", "cancelled"];

pub(super) fn validate_status(status: &str) -> AppResult<()> {
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
