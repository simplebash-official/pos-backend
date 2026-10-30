use super::super::db::Db;

use super::backend::Backend;
use super::mongo_tx::MongoTx;
use super::sqlite_conn::SqliteConn;
use super::tx::Tx;
use super::tx_engine::TxEngine;
use crate::core::error::AppResult;

/// Where a repository call runs: straight on the database, or inside a `Tx`.
#[derive(Debug)]
pub enum Unit<'a> {
    Db(&'a Db),
    Tx(&'a mut Tx),
}

impl<'a> From<&'a Db> for Unit<'a> {
    fn from(db: &'a Db) -> Self {
        Unit::Db(db)
    }
}

impl<'a> From<&'a mut Tx> for Unit<'a> {
    fn from(tx: &'a mut Tx) -> Self {
        Unit::Tx(tx)
    }
}

/// Lets a service pass the unit it received on to several repository calls.
impl<'a, 'b> From<&'a mut Unit<'b>> for Unit<'a> {
    fn from(unit: &'a mut Unit<'b>) -> Self {
        match unit {
            Unit::Db(db) => Unit::Db(db),
            Unit::Tx(tx) => Unit::Tx(tx),
        }
    }
}

impl Unit<'_> {
    pub async fn backend(&mut self) -> AppResult<Backend<'_>> {
        match self {
            Unit::Db(Db::Mongo(db)) => Ok(Backend::Mongo(db, None)),
            Unit::Db(Db::Sqlite(pool)) => {
                Ok(Backend::Sqlite(SqliteConn::Pooled(pool.acquire().await?)))
            }
            Unit::Tx(tx) => match &mut tx.engine {
                TxEngine::Sqlite(tx) => Ok(Backend::Sqlite(SqliteConn::Tx(tx))),
                TxEngine::Mongo(mongo) => {
                    let MongoTx { db, session } = &mut **mongo;
                    Ok(Backend::Mongo(db, Some(session)))
                }
            },
        }
    }
}
