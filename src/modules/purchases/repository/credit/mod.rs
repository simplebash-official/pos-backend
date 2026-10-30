//! Dual-engine (Mongo / SQLite) access for the `supplier_credits` collection. Append-only:
//! credits are inserted and read, never edited or deleted; a void is another row.

mod repository_credit;

pub(crate) use repository_credit::find_credit_by_key;
pub(crate) use repository_credit::insert_credit;
pub(crate) use repository_credit::list_credits_for_receipts;
pub(crate) use repository_credit::list_credits_for_supplier;
pub(crate) use repository_credit::list_credits_paginated;
