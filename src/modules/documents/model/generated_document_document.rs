use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Mongo document shape for tracking generated PDF/print documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GeneratedDocumentDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this generated document record.
    pub key: String,
    /// Key of the entity this document was rendered for (e.g. invoice key).
    pub entity_key: String,
    /// Document layout type (e.g. "a4-invoice", "thermal-receipt").
    pub document_type: String,
    /// Document template name used for rendering.
    pub template_name: String,
    /// The document-server-minted unique template key (`tpl_...`) the PDF
    /// was rendered against — the POS side durably holding the ID that
    /// service assigned, per the integration contract. `default` for rows
    /// written before this field existed.
    pub template_key: String,
    /// The document-server template name (`doc_temp_...`, one per design
    /// version) the PDF was rendered with. `None` only for rows written before
    /// designs were recorded.
    pub template_ref: Option<String>,
    /// That template's title when it was rendered.
    pub template_title: Option<String>,
    /// That template's design-family version when it was rendered.
    pub template_version: Option<i64>,
    /// Relative filesystem path to the saved PDF file.
    pub file_path: String,
    /// Size of the generated PDF document in bytes.
    pub file_size_bytes: i64,
    /// Timestamp when this document record was created.
    pub created_at: BsonDateTime,
    /// Timestamp when this document record was last updated.
    pub updated_at: BsonDateTime,
}
