//! HTTP layer only: extractors, path/query parsing, and OpenAPI docs
//! (`#[utoipa::path]`). Every handler follows the same shape — extract
//! params, make exactly one `service::*` call, wrap the result in
//! `ApiResponse`/`StatusCode` — so individual handlers aren't commented
//! beyond that pattern; the business logic they call into lives in
//! `service/` and is commented there. Unlike most other modules, there's no
//! placeholder `GET /` status route here — the collection root (`GET /`,
//! nested at `/api/suppliers`) is itself the real "list suppliers" endpoint
//! from day one, so there's no unused path left for a status ping. Every
//! route — reads included — requires `AdminUser`: supplier data (pricing,
//! contacts) is treated as business-sensitive, not general shop-floor data.
//! The one exception is `POST /` (register a supplier), which needs the
//! `suppliers:create` permission so a Manager can add one from the cash window.

mod suppliers_routes;

pub use suppliers_routes::router;
