// External service clients. Currently just MongoDB; `AppState` holds the
// connected `Database` handle built here so modules never reconnect.
pub mod indexes;
pub mod mongo;
