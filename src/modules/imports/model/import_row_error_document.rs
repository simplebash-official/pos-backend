use crate::domain::imports::ImportRowError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRowErrorDocument {
    pub row_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_value: Option<String>,
}

impl ImportRowErrorDocument {
    #[must_use]
    pub fn into_row_error(self) -> ImportRowError {
        ImportRowError {
            row_number: self.row_number,
            field: self.field,
            message: self.message,
            raw_value: self.raw_value,
        }
    }

    #[must_use]
    pub fn from_row_error(err: ImportRowError) -> Self {
        Self {
            row_number: err.row_number,
            field: err.field,
            message: err.message,
            raw_value: err.raw_value,
        }
    }
}
