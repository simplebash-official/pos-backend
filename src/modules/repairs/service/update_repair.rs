use super::resolve_assignment_employee_name::resolve_assignment_employee_name;
use super::resolve_customer_name_phone::resolve_customer_name_phone;
use super::validate_price_required_for_status::validate_price_required_for_status;
use super::validate_promised_ready_at::validate_promised_ready_at;
use super::validate_required_fields::validate_required_fields;
use super::validate_status::validate_status;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::repairs::{Repair, UpdateRepairRequest};

use crate::modules::repairs::repository;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn update_repair(
    db: &Db,
    id_or_key: &str,
    body: UpdateRepairRequest,
    device_id: Option<String>,
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
    })
    .await
}
