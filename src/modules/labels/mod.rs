// Label printing: shop-built label designs (`label_templates`), the ready-made
// starters, and the endpoints that turn products and serial numbers into a PDF
// of stickers. Same routes -> service -> repository layering as the other
// modules; `repository` stays private.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
