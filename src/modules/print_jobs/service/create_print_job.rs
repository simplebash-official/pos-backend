use super::resolve_assignment_employee_name::resolve_assignment_employee_name;
use super::resolve_customer_name_phone::resolve_customer_name_phone;
use super::validate_job_type::validate_job_type;
use super::validate_promised_ready_at::validate_promised_ready_at;
use super::validate_required_fields::validate_required_fields;
use super::validate_status::validate_status;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;

use crate::domain::print_jobs::{CreatePrintJobRequest, PrintJob, PrintJobAssignment};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::modules::print_jobs::model::PrintJobDocument;
use crate::modules::print_jobs::repository;
use crate::modules::sequences;
use mongodb::bson::DateTime as BsonDateTime;

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
    })
    .await
}
