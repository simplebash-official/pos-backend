// Dual-engine (Mongo / SQLite) access for the `supplier_credits` collection. Append-only:
// credits are inserted and read, never edited or deleted; a void is another row.

mod credit_from_row;
mod credits;
mod find_credit_by_key;
mod insert_credit;
mod list_credits_for_receipts;
mod list_credits_for_supplier;
mod list_credits_paginated;

pub(crate) use find_credit_by_key::find_credit_by_key;
pub(crate) use insert_credit::insert_credit;
pub(crate) use list_credits_for_receipts::list_credits_for_receipts;
pub(crate) use list_credits_for_supplier::list_credits_for_supplier;
pub(crate) use list_credits_paginated::list_credits_paginated;
