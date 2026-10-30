// SQLite client configuration, connection pooling, and schema bootstrap.

mod bson_to_iso;
mod connect;
mod derive_username;
mod generate_id_hex;
mod init_db;
mod map_sqlite_row_to_document;
mod migrate_cash_movements;
mod migrate_purchases_to_receipts;
mod migrate_schema;
mod migrate_to_stock_batches;
mod now_utc_iso;
mod parse_iso_datetime;
mod sql_filter;
mod table_columns;
mod to_bson_datetime;
#[cfg(test)]
mod username_migration_tests;

pub use bson_to_iso::bson_to_iso;
pub use connect::connect;
pub use generate_id_hex::generate_id_hex;
pub use init_db::init_db;
pub use map_sqlite_row_to_document::map_sqlite_row_to_document;
pub use migrate_cash_movements::migrate_cash_movements;
pub use migrate_purchases_to_receipts::migrate_purchases_to_receipts;
pub use migrate_schema::migrate_schema;
pub use migrate_to_stock_batches::migrate_to_stock_batches;
pub use now_utc_iso::now_utc_iso;
pub use parse_iso_datetime::parse_iso_datetime;
pub use sql_filter::SqlFilter;
pub use to_bson_datetime::to_bson_datetime;
