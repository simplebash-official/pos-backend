//! Batch import processing for Inventory Products. Validates row fields,
//! resolves category and subcategory names to keys, generates sequential
//! SKUs and barcodes, and inserts product documents and initial stock movements.

mod import_warranty;
mod inventory_import_result;
mod process_inventory_import;

pub(crate) use process_inventory_import::process_inventory_import;
