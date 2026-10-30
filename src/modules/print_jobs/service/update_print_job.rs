use super::resolve_assignment_employee_name::resolve_assignment_employee_name;
use super::resolve_customer_name_phone::resolve_customer_name_phone;
use super::validate_job_type::validate_job_type;
use super::validate_promised_ready_at::validate_promised_ready_at;
use super::validate_required_fields::validate_required_fields;
use super::validate_status::validate_status;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::print_jobs::{PrintJob, UpdatePrintJobRequest};

use crate::modules::print_jobs::repository;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn update_print_job(
    db: &Db,
    id_or_key: &str,
    body: UpdatePrintJobRequest,
    device_id: Option<String>,
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
        validate_job_type(&job_type)?;
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
    })
    .await
}
