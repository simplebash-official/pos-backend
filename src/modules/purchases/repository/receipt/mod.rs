//! Dual-engine (Mongo / SQLite) access for the `purchase_receipts` collection. Receipts are
//! append-only: inserted and read, never edited or deleted. Like the rest of this module, nothing
//! here turns a missing document into an error; `service` does that.

mod repository_receipt;

pub(crate) use repository_receipt::find_receipt_by_key;
pub(crate) use repository_receipt::find_receipt_by_supplier_and_reference;
pub(crate) use repository_receipt::insert_receipt;
pub(crate) use repository_receipt::list_receipts_paginated;
