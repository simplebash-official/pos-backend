// Receipt photos attached to a cash movement, expense or income. The image is stored with the row
// as base64 text so it travels through sync like any other record.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
