// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there. No placeholder status route here for
// the same reason as `suppliers`: the collection root is a real endpoint.
// Every route — reads included — requires `AdminUser`, matching `suppliers`.

mod delete_link;
mod list_links;
mod replace_links_for_supplier;
mod router;
mod upsert_link;

pub use router::router;
