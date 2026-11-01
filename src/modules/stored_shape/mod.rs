//! Completeness of stored MongoDB documents.
//!
//! Stored documents are read with no `#[serde(default)]`: a document that lacks a
//! required field fails to load instead of getting an invented value. Documents written
//! before a field existed therefore have to be completed once. `scan` finds them
//! (`bin/check_data_shape`) and, in apply mode, writes the missing fields
//! (`bin/migrate_stored_defaults`). It runs **before** a backend with this rule is
//! deployed to an environment with existing data.

mod stored_collection;
mod stored_shape_probe;
mod stored_shape_scan;

pub use stored_collection::{STORED_COLLECTIONS, StoredCollection};
pub use stored_shape_probe::{Fault, Repair, repair_document};
pub use stored_shape_scan::{CollectionShape, Mode, ShapeReport, scan};
