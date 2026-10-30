// Dual-engine (Mongo / SQLite) access for the `purchase_receipts` collection. Receipts are
// append-only: inserted and read, never edited or deleted. Like the rest of this module, nothing
// here turns a missing document into an error; `service` does that.

mod find_receipt_by_key;
mod find_receipt_by_supplier_and_reference;
mod insert_receipt;
mod list_receipts_paginated;
mod receipt_from_row;
mod receipts;

pub(crate) use find_receipt_by_key::find_receipt_by_key;
pub(crate) use find_receipt_by_supplier_and_reference::find_receipt_by_supplier_and_reference;
pub(crate) use insert_receipt::insert_receipt;
pub(crate) use list_receipts_paginated::list_receipts_paginated;
