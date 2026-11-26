use crate::domain::print_jobs::PrintService;
use crate::domain::warranties::ProductWarranty;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A stored print service (`print_service_templates`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintServiceDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub name: String,
    pub unit_price_cents: i64,
    /// Stored in its API shape (camelCase), like a product's warranty.
    pub warranty: ProductWarranty,
    pub active: bool,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl PrintServiceDocument {
    /// The service as clients see it.
    #[must_use]
    pub fn into_print_service(self) -> PrintService {
        PrintService {
            id: self
                .id
                .expect("a stored print service always has an id")
                .to_hex(),
            key: self.key,
            name: self.name,
            unit_price_cents: self.unit_price_cents,
            warranty: self.warranty,
            active: self.active,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            updated_by_device: self.updated_by_device,
        }
    }
}
