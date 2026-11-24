use super::repair_alerts;
use super::repair_items::{price_items, use_parts};
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::utils::{build_bson_regex, calculate_pagination, shop_today_range};
use crate::domain::billing::Handover;
use crate::domain::inventory::StockMovementType;
use crate::domain::repairs::{
    CreateRepairRequest, Repair, RepairAssignment, RepairItem, RepairKind, RepairListQuery,
    RepairListResponse, RepairPart, RepairStats, UpdateRepairRequest,
};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::modules::repairs::model::RepairDocument;
use crate::modules::repairs::repository;
use crate::modules::warranties::service::WarrantyActor;
use crate::modules::{billing, customers, employees, sequences, warranties};
use chrono::Utc;
use mongodb::bson::{Bson, DateTime as BsonDateTime, Document, doc};

/// Creates the repair.
pub async fn create_repair(
    db: &Db,
    body: CreateRepairRequest,
    device_id: Option<String>,
) -> AppResult<Repair> {
    crate::core::logging::domain::tracked("repairs.created", async move {
        let status = body.status.unwrap_or_else(|| "received".to_string());
        validate_status(&status)?;
        validate_promised_ready_at(body.promised_ready_at.as_deref())?;

        let customer =
            customers::service::get_customer_by_key(db, &body.customer.customer_key).await?;

        validate_required_fields(&body.device_model, &body.issue_description)?;
        let kind = RepairKind::Standard;
        let priced = price_items(db, body.items, &[], &kind).await?;
        validate_price_required_for_status(&status, &kind, &priced.items)?;
        if status == "delivered" {
            refuse_handing_back_unbilled_parts(&kind, &priced.items, None)?;
        }

        let assignment = body.assignment.unwrap_or_default();
        let RepairAssignment {
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

        let ticket_number = next_ticket_number(db, device_id.clone()).await?;

        let now = BsonDateTime::now();
        let document = RepairDocument {
            id: None,
            key: generate_id(prefixes::REPAIR),
            ticket_number,
            customer_key: customer.key,
            customer_name: customer.name,
            customer_phone: customer.primary_phone,
            device_model: body.device_model,
            serial_number: body.serial_number,
            issue_description: body.issue_description,
            promised_ready_at: body.promised_ready_at,
            status,
            kind,
            items: priced.items,
            price_cents: priced.price_cents,
            cost_cents: priced.cost_cents,
            assigned_employee_id,
            assigned_employee_name,
            split_type,
            split_value,
            billed_invoice_key: None,
            billed_invoice_number: None,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: device_id,
        };

        let inserted = repository::insert(db, document).await?;
        Ok(inserted.into_repair())
    })
    .await
}

/// The next `REP-000001` number.
async fn next_ticket_number(db: &Db, device_id: Option<String>) -> AppResult<String> {
    let reservation = sequences::service::reserve_sequence(
        db,
        "repair".to_string(),
        ReserveSequenceRequest {
            block_size: Some(1),
            device_id,
        },
    )
    .await?;
    Ok(format!(
        "{}{:0width$}",
        reservation.prefix,
        reservation.start,
        width = reservation.padding
    ))
}

/// What a warranty claim on a repair needs to open its rework job.
#[derive(Debug)]
pub(crate) struct NewRework<'a> {
    pub claim_key: &'a str,
    pub claim_number: &'a str,
    pub original_repair_key: &'a str,
    /// What the customer says is wrong now.
    pub issue_description: &'a str,
    pub assigned_employee_key: Option<&'a str>,
    pub device_id: Option<String>,
}

/// Opens a free warranty rework of an earlier job: same customer and device, no items yet (the
/// technician adds the work and any parts used), and a `WarrantyRework` kind that links back to
/// the claim and the original job.
pub(crate) async fn create_rework(db: &Db, rework: NewRework<'_>) -> AppResult<Repair> {
    crate::core::logging::domain::tracked("repairs.rework_created", async move {
        let original = repository::find_by_id_or_key(db, rework.original_repair_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
            })?;
        let assigned_employee_name = match rework.assigned_employee_key {
            Some(key) => Some(
                employees::service::resolve_assignment(db, key, None, None)
                    .await?
                    .name,
            ),
            None => None,
        };
        let ticket_number = next_ticket_number(db, rework.device_id.clone()).await?;
        let now = BsonDateTime::now();
        let document = RepairDocument {
            id: None,
            key: generate_id(prefixes::REPAIR),
            ticket_number,
            customer_key: original.customer_key,
            customer_name: original.customer_name,
            customer_phone: original.customer_phone,
            device_model: original.device_model,
            serial_number: original.serial_number,
            issue_description: rework.issue_description.to_string(),
            promised_ready_at: None,
            status: "received".to_string(),
            kind: RepairKind::WarrantyRework {
                claim_key: rework.claim_key.to_string(),
                claim_number: rework.claim_number.to_string(),
                original_repair_key: original.key,
                original_ticket_number: original.ticket_number,
            },
            items: Vec::new(),
            price_cents: 0,
            cost_cents: 0,
            assigned_employee_id: rework.assigned_employee_key.map(str::to_string),
            assigned_employee_name,
            split_type: None,
            split_value: None,
            billed_invoice_key: None,
            billed_invoice_number: None,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: rework.device_id,
        };
        Ok(repository::insert(db, document).await?.into_repair())
    })
    .await
}

/// Deletes the repair.
pub async fn delete_repair(
    db: &Db,
    id_or_key: &str,
    device_id: Option<String>,
) -> AppResult<Repair> {
    crate::core::logging::domain::tracked("repairs.deleted", async move {
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
    })
    .await
}

/// Returns the repair.
pub async fn get_repair(db: &Db, id_or_key: &str) -> AppResult<Repair> {
    let document = repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
        })?;
    Ok(document.into_repair())
}

/// Computes the KPI cards for the Repair Jobs screen. See `shop_today_range`
/// for how "today" is bounded.
pub async fn get_repair_stats(db: &Db) -> AppResult<RepairStats> {
    let (today_start, today_end) = shop_today_range();
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

/// Lists the repairs.
pub async fn list_repairs(db: &Db, query: RepairListQuery) -> AppResult<RepairListResponse> {
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

/// Cross-module hook, called by `billing::service::sale::complete_sale` when an invoice bills a
/// repair job. Narrow on purpose: only stamps the job with the invoice that billed it and, for a
/// `Handover::Now` line, sets `status` to `delivered`. It never touches items or assignment, since
/// a sale completing shouldn't silently rewrite details a technician entered. A job that is
/// already billed is a 409 (`TICKET_ALREADY_BILLED`), so of two sales racing for one job only one
/// wins. The caller then takes the parts out of stock (`use_parts`) and, for `Now`, issues the
/// items' warranties.
pub(crate) async fn mark_billed(
    db: &Db,
    key: &str,
    invoice_key: &str,
    invoice_number: &str,
    handover: Handover,
) -> AppResult<Repair> {
    crate::core::logging::domain::tracked("repairs.billed", async move {
        let status = match handover {
            Handover::Now => Some("delivered"),
            Handover::Later => None,
        };
        if let Some(updated) =
            repository::stamp_billed(db, key, invoice_key, invoice_number, status).await?
        {
            let repair = updated.into_repair();
            if handover == Handover::Now {
                // The status before the write is gone, so only "delivered" is compared: a job
                // billed twice raises one notice (the dedupe key holds the status).
                repair_alerts::announce_status_change(db, &repair, "").await?;
            }
            return Ok(repair);
        }
        let existing = repository::find_by_id_or_key(db, key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
            })?;
        Err(AppError::conflict(
            codes::TICKET_ALREADY_BILLED,
            format!(
                "Repair ticket {} was already billed on {}",
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
/// the voided invoice left on a repair job, so the job can be billed again. Leaves the status and
/// the parts alone. Returns whether the job still carried that invoice's mark.
pub(crate) async fn clear_billed(db: &Db, key: &str, invoice_key: &str) -> AppResult<bool> {
    crate::core::logging::domain::tracked("repairs.unbilled", async move {
        repository::clear_billed_stamp(db, key, invoice_key).await
    })
    .await
}

/// Updates the repair.
pub(crate) async fn update_repair(
    db: &Db,
    id_or_key: &str,
    body: UpdateRepairRequest,
    device_id: Option<String>,
    actor: &WarrantyActor,
) -> AppResult<Repair> {
    crate::core::logging::domain::tracked("repairs.updated", async move {
        let existing = repository::find_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
            })?;
        let object_id = existing
            .id
            .expect("persisted repair document must have an _id");

        let customer_key = match &body.customer {
            Some(customer) => customer.customer_key.as_str(),
            None => existing.customer_key.as_str(),
        };
        let customer = customers::service::get_customer_by_key(db, customer_key).await?;
        let device_model = body.device_model.unwrap_or(existing.device_model);
        let issue_description = body.issue_description.unwrap_or(existing.issue_description);
        let serial_after = body
            .serial_number
            .clone()
            .or_else(|| existing.serial_number.clone());
        let was_delivered = existing.status == "delivered";
        let previous_status = existing.status.clone();
        let status = body.status.unwrap_or(existing.status);
        let billed_invoice_key = existing.billed_invoice_key.as_deref();

        validate_status(&status)?;
        validate_promised_ready_at(body.promised_ready_at.as_deref())?;
        validate_required_fields(&device_model, &issue_description)?;
        let priced = match body.items {
            Some(_) if billed_invoice_key.is_some() => {
                return Err(AppError::conflict(
                    codes::REPAIR_ITEMS_LOCKED,
                    "This job has been billed; its work can no longer be changed. Void the \
                     invoice to change it.",
                ));
            }
            Some(_) if was_delivered => {
                return Err(AppError::conflict(
                    codes::REPAIR_ITEMS_LOCKED,
                    "This job has been handed back; its work can no longer be changed.",
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
            "device_model": &device_model,
            "issue_description": &issue_description,
            "status": &status,
            "updated_at": BsonDateTime::now(),
        };
        if let Some(priced) = &priced {
            set_doc.insert("items", mongodb::bson::serialize_to_bson(&priced.items)?);
            set_doc.insert("price_cents", priced.price_cents);
            set_doc.insert("cost_cents", priced.cost_cents);
        }
        if let Some(sn) = body.serial_number {
            set_doc.insert("serial_number", sn);
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
                    repair_key: &existing.key,
                    ticket_number: &existing.ticket_number,
                    device_model: &device_model,
                    serial_number: serial_after.as_deref(),
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
                AppError::not_found_with_code("Repair ticket not found", codes::REPAIR_NOT_FOUND)
            })?
            .into_repair();
        repair_alerts::announce_status_change(db, &updated, &previous_status).await?;
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
    kind: &RepairKind,
    items: &[RepairItem],
    billed_invoice_key: Option<&str>,
) -> AppResult<()> {
    let has_stock_part = items
        .iter()
        .any(|item| matches!(item.part, Some(RepairPart::Stock { .. })));
    if !kind.is_rework() && has_stock_part && billed_invoice_key.is_none() {
        return Err(AppError::validation_with_code(
            "Bill this job to hand it back: its parts from stock leave stock with the invoice.",
            codes::REPAIR_PART_INVALID,
        ));
    }
    Ok(())
}

/// The job being handed back, as its warranties need it.
#[derive(Debug)]
struct HandedBack<'a> {
    repair_key: &'a str,
    ticket_number: &'a str,
    device_model: &'a str,
    serial_number: Option<&'a str>,
    items: &'a [RepairItem],
}

/// Gives the items of a job billed with the hand-over later their warranties, starting today, from
/// the invoice that billed it (its number, its line and its customer). Does nothing when that
/// invoice already gave the job warranties: a job reopened and handed back again keeps its first
/// ones. A rework is never billed, so it never gets here.
async fn issue_handback_warranties(
    db: &Db,
    job: HandedBack<'_>,
    invoice_key: &str,
    actor: &WarrantyActor,
    device_id: Option<String>,
) -> AppResult<()> {
    if warranties::service::exist_for_repair_on_invoice(db, job.repair_key, invoice_key).await? {
        return Ok(());
    }
    let invoice = billing::service::get_invoice(db, invoice_key).await?;
    let line_index = invoice
        .items
        .iter()
        .position(|line| line.source_ticket_key.as_deref() == Some(job.repair_key))
        .ok_or_else(|| {
            AppError::internal(format!(
                "invoice {} does not bill repair {}, which names it as its invoice",
                invoice.invoice_number, job.ticket_number
            ))
        })?;
    warranties::service::issue_for_repair(
        db,
        warranties::service::RepairLine {
            invoice_key: &invoice.key,
            invoice_number: &invoice.invoice_number,
            delivered_at: Utc::now(),
            customer_key: invoice.customer_key.as_deref(),
            customer_name: invoice.customer_name_snapshot.as_deref(),
            customer_phone: invoice.customer_phone_snapshot.as_deref(),
            line_index,
            repair_key: job.repair_key,
            ticket_number: job.ticket_number,
            device_model: job.device_model,
            device_serial: job.serial_number,
            items: job.items,
            actor,
            device_id,
        },
    )
    .await?;
    Ok(())
}

/// A ticket may sit with no items while `received`, `diagnosing`, `in_repair`
/// or `cancelled`; diagnosis-first repairs often can't be quoted before or
/// during repair work. Once it moves to `ready` or `delivered` it needs at
/// least one item, since those statuses mean the ticket is headed to
/// pickup/billing. A warranty rework charges nothing, so it needs none.
/// Called on the *effective* post-write status/items so a single PATCH that
/// sets both together is validated against the combined result.
const STATUSES_REQUIRING_PRICE: &[&str] = &["ready", "delivered"];

pub(super) fn validate_price_required_for_status(
    status: &str,
    kind: &RepairKind,
    items: &[RepairItem],
) -> AppResult<()> {
    if STATUSES_REQUIRING_PRICE.contains(&status) && !kind.is_rework() && items.is_empty() {
        return Err(AppError::validation_with_code(
            format!(
                "Add the work done before moving a ticket to '{status}'. A job needs at least one item."
            ),
            codes::REPAIR_PRICE_REQUIRED,
        ));
    }
    Ok(())
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

pub(super) fn validate_required_fields(
    device_model: &str,
    issue_description: &str,
) -> AppResult<()> {
    if device_model.trim().is_empty() {
        return Err(AppError::validation("Device model is required"));
    }
    if issue_description.trim().is_empty() {
        return Err(AppError::validation("Issue description is required"));
    }
    Ok(())
}

const VALID_STATUSES: &[&str] = &[
    "received",
    "diagnosing",
    "in_repair",
    "ready",
    "delivered",
    "cancelled",
];

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
