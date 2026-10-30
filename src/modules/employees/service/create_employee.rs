use super::validate_required_fields::validate_required_fields;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;
use crate::domain::employees::{CreateEmployeeRequest, Employee, EmployeeStatus};
use crate::modules::employees::model::EmployeeDocument;
use crate::modules::employees::repository;
use mongodb::bson::DateTime as BsonDateTime;

pub async fn create_employee(db: &Db, body: CreateEmployeeRequest) -> AppResult<Employee> {
    crate::core::logging::domain::tracked("employees.created", async move {
        validate_required_fields(&body.name, &body.phone, body.default_split_value)?;

        let now = BsonDateTime::now();
        let document = EmployeeDocument {
            id: None,
            key: generate_id(prefixes::EMPLOYEE),
            name: body.name,
            phone: body.phone,
            nic_or_id: body.nic_or_id,
            role: body.role,
            default_split_type: body.default_split_type,
            default_split_value: body.default_split_value,
            status: body.status.unwrap_or(EmployeeStatus::Active),
            notes: body.notes,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        };

        let inserted = repository::insert_employee(db, document).await?;
        Ok(inserted.into_employee(None))
    })
    .await
}
