// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there. No placeholder status route here for
// the same reason as `suppliers`/`supplier_products`: the collection root
// is a real endpoint. Every route — reads included — requires `AdminUser`,
// matching `suppliers`/`supplier_products`.

mod credits_router;
mod get_receipt;
mod list_credits;
mod list_purchases;
mod list_receipts;
mod receipts_router;
mod record_credit;
mod record_receipt;
mod router;
mod void_credit;

pub use credits_router::credits_router;
pub use receipts_router::receipts_router;
pub use router::router;
