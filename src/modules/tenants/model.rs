// Mongo document shape for the platform `tenants` directory.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Immutable tenant id (`tnt_<nanoid>`); this is what `tenant_id` holds on
    /// every tenant-owned document and what the JWT `tid` claim carries.
    pub key: String,
    /// Lowercase login slug, globally unique.
    pub shop_code: String,
    /// Display name of the business.
    pub name: String,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Whether initial onboarding setup has been completed for this tenant.
    #[serde(default)]
    pub setup_completed: bool,
    /// Whether demo sample data was loaded during onboarding setup.
    #[serde(default)]
    pub sample_data_loaded: bool,
    /// Timestamp when onboarding setup was completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_completed_at: Option<BsonDateTime>,
}
