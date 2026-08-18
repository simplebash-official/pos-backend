// Mongo document shape for the `generated_documents` collection — metadata
// only, never PDF bytes (those live on disk, see `service::get_or_render`).
// No `into_*` conversion into a `domain` DTO exists because nothing outside
// this module currently reads this shape as JSON; `billing`'s document
// routes (Phase 4) stream the file straight off disk, not this record.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

/// Mongo document shape for tracking generated PDF/print documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GeneratedDocumentDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this generated document record.
    #[serde(default)]
    pub key: String,
    /// Key of the entity this document was rendered for (e.g. invoice key).
    pub entity_key: String,
    /// Document layout type (e.g. "a4-invoice", "thermal-receipt").
    pub document_type: String,
    /// Document template name used for rendering.
    pub template_name: String,
    /// Relative filesystem path to the saved PDF file.
    pub file_path: String,
    /// Size of the generated PDF document in bytes.
    pub file_size_bytes: i64,
    /// Timestamp when this document record was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when this document record was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}
