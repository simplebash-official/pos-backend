//! Dual-engine (Mongo / SQLite) access for the `receipt_charges` collection: the extra charges
//! (shipping, duty, handling, insurance, discounts) on a receipt. Append-only: inserted and read,
//! never edited or deleted.

mod repository_charge;

pub(crate) use repository_charge::insert_charge;
pub(crate) use repository_charge::list_charges_for_receipts;
pub(crate) use repository_charge::list_supplier_billed_charges;
