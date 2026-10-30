//! HTTP layer only: extractors, permission checks and OpenAPI docs. Every
//! handler makes one `service::*` call; the rules live in `service/`.

mod labels_routes;

pub use labels_routes::LABEL_JOB_KEY_HEADER;
pub use labels_routes::router;
