//! HTTP client for the sibling document-server (Typst PDF rendering,
//! `../document-server`). This is the *only* caller of that service anywhere
//! in the system — the frontend never talks to it directly (see
//! `modules::documents::service`, which is the only module allowed to call
//! this client). Built once at startup and shared read-only via `AppState`,
//! the same "connect/build once, hand out a shared handle" shape as
//! `clients::mongo::connect`.

mod document_server_client;
mod template_info;

pub use document_server_client::DocumentServerClient;
pub use template_info::TemplateInfo;
