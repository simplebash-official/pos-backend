use crate::clients::sqlite::to_bson_datetime;
use crate::domain::employees::{EmployeeRole, EmployeeStatus, SplitType};
use crate::modules::employees::model::EmployeeDocument;
use mongodb::bson::oid::ObjectId;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct EmployeeSqliteRow {
    pub(super) id: String,
    pub(super) key: String,
    pub(super) name: String,
    pub(super) phone: String,
    pub(super) nic_or_id: Option<String>,
    pub(super) role: String,
    pub(super) default_split_type: String,
    pub(super) default_split_value: f64,
    pub(super) status: String,
    pub(super) notes: Option<String>,
    pub(super) version: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) deleted_at: Option<String>,
    pub(super) updated_by_device: Option<String>,
}

impl EmployeeSqliteRow {
    pub(super) fn into_document(self) -> EmployeeDocument {
        EmployeeDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            name: self.name,
            phone: self.phone,
            nic_or_id: self.nic_or_id,
            role: self.role.parse().unwrap_or(EmployeeRole::General),
            default_split_type: self
                .default_split_type
                .parse()
                .unwrap_or(SplitType::Percentage),
            default_split_value: self.default_split_value,
            status: self.status.parse().unwrap_or(EmployeeStatus::Active),
            notes: self.notes,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}
