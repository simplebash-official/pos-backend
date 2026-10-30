use crate::domain::imports::ImportRowError;

/// Result of processing an inventory batch import.
#[derive(Debug)]
pub struct InventoryImportResult {
    pub successful_count: u64,
    pub failed_count: u64,
    pub errors: Vec<ImportRowError>,
}
