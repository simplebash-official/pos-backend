use super::mongo_tx::MongoTx;

use sqlx::{Sqlite, Transaction};

#[derive(Debug)]
pub(super) enum TxEngine {
    Sqlite(Transaction<'static, Sqlite>),
    // Boxed: a driver session is far larger than a SQLite transaction handle.
    Mongo(Box<MongoTx>),
}
