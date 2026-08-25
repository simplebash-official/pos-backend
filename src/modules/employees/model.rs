// Mongo document shape for the employees feature. Kept separate from
// `domain::employees` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads. Shaped like
// `modules::suppliers::model::SupplierDocument` (version/soft-delete/
// updated_by_device, sync-ready) rather than `modules::users::model::
// UserDocument` (no version, hard delete) — an Employee is a synced
// resource (see `modules::sync::service::SYNCABLE`), a User login never is.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::employees::{Employee, EmployeeLoginSummary, EmployeeRole, EmployeeStatus, SplitType},
};

fn default_version() -> i64 {
    1
}

fn default_status() -> EmployeeStatus {
    EmployeeStatus::Active
}

/// Mongo document shape representing an employee HR/commission profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeeDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub name: String,
    pub phone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nic_or_id: Option<String>,
    pub role: EmployeeRole,
    pub default_split_type: SplitType,
    pub default_split_value: f64,
    #[serde(default = "default_status")]
    pub status: EmployeeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl EmployeeDocument {
    /// Converts to the API-facing `Employee` shape. `login` is passed in by
    /// the caller (`service`) rather than resolved here, since finding it
    /// requires a cross-module DB call this document type can't make itself
    /// — the one deviation from `SupplierDocument::into_supplier()`.
    pub fn into_employee(self, login: Option<EmployeeLoginSummary>) -> Employee {
        let key = if self.key.is_empty() {
            generate_id(prefixes::EMPLOYEE)
        } else {
            self.key
        };
        Employee {
            id: self
                .id
                .expect("persisted employee document must have an id")
                .to_hex(),
            key,
            name: self.name,
            phone: self.phone,
            nic_or_id: self.nic_or_id,
            role: self.role,
            default_split_type: self.default_split_type,
            default_split_value: self.default_split_value,
            status: self.status,
            notes: self.notes,
            login,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
