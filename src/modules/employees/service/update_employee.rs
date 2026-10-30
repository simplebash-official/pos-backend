use super::validate_required_fields::validate_required_fields;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::employees::{Employee, UpdateEmployeeRequest};
use crate::modules::employees::repository;
use crate::modules::users;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Partial update for `PATCH /employees/{id}` — every field in `body` is
/// optional; required fields (name/phone/defaultSplitValue) fall back to the
/// existing document's value before re-validation, while genuinely-optional
/// fields (nicOrId/notes) are only touched when explicitly provided (same
/// convention as `suppliers::service::update_supplier`).
pub(crate) async fn update_employee(
    db: &Db,
    id: ObjectId,
    body: UpdateEmployeeRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Employee> {
    crate::core::logging::domain::tracked("employees.updated", async move {
        let existing = repository::find_employee_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
            })?;

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            let mut conflicting = Vec::new();
            if let Some(ref name) = body.name
                && name != &existing.name
            {
                conflicting.push("name");
            }
            if let Some(ref phone) = body.phone
                && phone != &existing.phone
            {
                conflicting.push("phone");
            }
            if let Some(ref split_value) = body.default_split_value
                && *split_value != existing.default_split_value
            {
                conflicting.push("defaultSplitValue");
            }

            let login =
                users::service::find_user_summary_by_employee_key(db, &existing.key).await?;
            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This employee was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": existing.into_employee(login),
                    "conflictingFields": conflicting,
                }),
            ));
        }

        let name = body.name.unwrap_or(existing.name);
        let phone = body.phone.unwrap_or(existing.phone);
        let default_split_value = body
            .default_split_value
            .unwrap_or(existing.default_split_value);

        validate_required_fields(&name, &phone, default_split_value)?;

        let mut set_doc = doc! {
            "name": &name,
            "phone": &phone,
            "default_split_value": default_split_value,
            "updated_at": BsonDateTime::now(),
        };
        if let Some(nic_or_id) = body.nic_or_id {
            set_doc.insert("nic_or_id", nic_or_id);
        }
        if let Some(role) = body.role {
            set_doc.insert("role", role.as_str());
        }
        if let Some(split_type) = body.default_split_type {
            set_doc.insert("default_split_type", split_type.as_str());
        }
        if let Some(status) = body.status {
            set_doc.insert("status", status.as_str());
        }
        if let Some(notes) = body.notes {
            set_doc.insert("notes", notes);
        }
        if let Some(device) = device_id {
            set_doc.insert("updated_by_device", device);
        }

        let updated = repository::update_employee(db, id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
            })?;
        let login = users::service::find_user_summary_by_employee_key(db, &updated.key).await?;

        Ok(updated.into_employee(login))
    })
    .await
}
