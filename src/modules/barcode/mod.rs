// Owns only a namespaced Mongo auto-increment counter and EAN-13 encoding
// logic — it has no notion of what a "Product" is, so the same engine can
// serve other label-worthy entities later (repair tickets, print job tags)
// without a rewrite. Unlike every other feature module, `barcode` has no
// HTTP surface: it's a pure cross-module utility reached exclusively via
// `pub(crate)` functions in `service` (mirrors how
// `inventory::repository::sku_counter` has no routes/domain of its own,
// just promoted to a full peer module since it's meant to be shared).
// `model`/`repository` stay private so nothing outside this tree — not even
// `inventory` — can reach past `service` into persistence.
mod model;
mod repository;
pub mod service;
