// Sale completion, voiding, and closing — the centerpiece of the billing
// backend migration. `complete_sale` implements D4 from the migration
// plan: all validation/lookups happen before any writes, and once the
// `Invoice` (+ payment, if any) is durably inserted, no later sub-step
// failure (stock decrement, ticket status update, customer balance update)
// makes the request report as failed — each collects into `warnings`
// instead. This mirrors `purchases::service::purchase::record_purchase`'s
// accepted risk posture: no Mongo transactions exist anywhere in this
// codebase, so a partial failure here is the same class of exposure that
// module already has between "purchase inserted" and "stock bumped".

mod close_invoice;
mod complete_sale;
mod resolve_sale_item;
mod resolve_sale_items;
mod void_invoice;

pub use close_invoice::close_invoice;
pub use complete_sale::complete_sale;

pub(crate) use resolve_sale_items::resolve_sale_items;
pub use void_invoice::void_invoice;
