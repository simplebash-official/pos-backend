//! External service clients: MongoDB, and the sibling document-server
//! (Typst PDF rendering). `AppState` holds handles built here so modules
//! never reconnect/reconstruct a client themselves.

mod connect_from_config;
pub mod db;
pub mod document_server;
pub mod indexes;
pub mod mongo;
pub mod sqlite;
pub mod sync_capture;
pub mod tenant_db;
pub mod uow;

pub use connect_from_config::connect_from_config;
