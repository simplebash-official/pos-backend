use crate::domain::repairs::RepairService;
use crate::domain::warranties::ProductWarranty;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A stored repair service (`repair_service_templates`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairServiceDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub name: String,
    pub labor_cents: i64,
    /// Stored in its API shape (camelCase), like a product's warranty.
    pub warranty: ProductWarranty,
    pub active: bool,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl RepairServiceDocument {
    /// The service as clients see it.
    #[must_use]
    pub fn into_repair_service(self) -> RepairService {
        RepairService {
            id: self
                .id
                .expect("a stored repair service always has an id")
                .to_hex(),
            key: self.key,
            name: self.name,
            labor_cents: self.labor_cents,
            warranty: self.warranty,
            active: self.active,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            updated_by_device: self.updated_by_device,
        }
    }
}
