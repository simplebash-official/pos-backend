//! SQLite client configuration, connection pooling, and schema bootstrap.

mod clients_sqlite;
mod migrate_schema;
#[cfg(test)]
mod username_migration_tests;

pub use clients_sqlite::SqlFilter;
pub use clients_sqlite::bson_to_iso;
pub use clients_sqlite::connect;
pub use clients_sqlite::generate_id_hex;
pub use clients_sqlite::init_db;
pub use clients_sqlite::map_sqlite_row_to_document;
pub use clients_sqlite::migrate_cash_movements;
pub use clients_sqlite::migrate_purchases_to_receipts;
pub use clients_sqlite::migrate_to_stock_batches;
pub use clients_sqlite::now_utc_iso;
pub use clients_sqlite::parse_iso_datetime;
pub use clients_sqlite::to_bson_datetime;
pub use migrate_schema::migrate_schema;
