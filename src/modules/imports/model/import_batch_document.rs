use super::import_row_error_document::ImportRowErrorDocument;
use crate::domain::imports::ImportBatch;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportBatchDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub target: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size_bytes: i64,
    pub total_rows: u64,
    pub successful_rows: u64,
    pub failed_rows: u64,
    pub status: String,
    #[serde(default)]
    pub errors: Vec<ImportRowErrorDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_name: Option<String>,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl ImportBatchDocument {
    pub fn into_import_batch(self) -> ImportBatch {
        ImportBatch {
            id: self.id.map(|oid| oid.to_hex()).unwrap_or_default(),
            key: self.key,
            target: self.target,
            file_name: self.file_name,
            file_type: self.file_type,
            file_size_bytes: self.file_size_bytes,
            total_rows: self.total_rows,
            successful_rows: self.successful_rows,
            failed_rows: self.failed_rows,
            status: self.status,
            errors: self
                .errors
                .into_iter()
                .map(ImportRowErrorDocument::into_row_error)
                .collect(),
            created_by_user_key: self.created_by_user_key,
            created_by_user_name: self.created_by_user_name,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
