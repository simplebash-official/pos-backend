use super::super::db::Db;

use super::mongo_tx::MongoTx;
use super::tx::Tx;
use super::tx_engine::TxEngine;
use crate::core::error::AppResult;

impl Db {
    /// Opens a transaction. SQLite takes the write lock up front
    /// (`BEGIN IMMEDIATE`) so a later statement cannot fail half-way on a lock.
    pub async fn begin(&self) -> AppResult<Tx> {
        match self {
            Db::Sqlite(pool) => Ok(Tx {
                engine: TxEngine::Sqlite(pool.begin_with("BEGIN IMMEDIATE").await?),
            }),
            Db::Mongo(db) => {
                let mut session = db.client().start_session().await?;
                session.start_transaction().await?;
                Ok(Tx {
                    engine: TxEngine::Mongo(Box::new(MongoTx {
                        db: db.clone(),
                        session,
                    })),
                })
            }
        }
    }
}
