// Mongo document shape for the `generated_documents` collection — metadata
// only, never PDF bytes (those live on disk, see `service::get_or_render`).
// No `into_*` conversion into a `domain` DTO exists because nothing outside
// this module currently reads this shape as JSON; `billing`'s document
// routes (Phase 4) stream the file straight off disk, not this record.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GeneratedDocumentDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    /// The key of the entity this document was rendered for (an invoice's
    /// `key`, so far — the only caller today). Not a strict foreign key
    /// constraint at the Mongo level, same convention as every other
    /// `*_key` field in this codebase.
    pub entity_key: String,
    /// e.g. `"a4-invoice"` / `"thermal-receipt"` — matches the
    /// document-server template name this was rendered from.
    pub document_type: String,
    /// The document-server template name actually used, kept alongside
    /// `document_type` (currently always equal) so a future template
    /// rename doesn't retroactively relabel history.
    pub template_name: String,
    /// Path to the saved PDF, relative to `Config::generated_documents_dir`.
    pub file_path: String,
    pub file_size_bytes: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}
