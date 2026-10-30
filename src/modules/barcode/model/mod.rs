//! Mongo document shape for the `barcode` module's counter table. This is a
//! pure internal sequence table, not a business entity — like
//! `inventory::repository::sku_counter::SkuCounterDocument`, it deliberately
//! has no `key`/`created_at`/`updated_at` fields (see CLAUDE.md's documented
//! exception for counter/sequence tables): its `_id` (`namespace`) already
//! is the meaningful key.

mod barcode_counter_document;

pub(crate) use barcode_counter_document::BarcodeCounterDocument;
