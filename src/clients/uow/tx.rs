use super::tx_engine::TxEngine;
use crate::core::error::AppResult;

/// An open transaction. Created by [`Db::begin`].
#[derive(Debug)]
pub struct Tx {
    pub(super) engine: TxEngine,
}

impl Tx {
    /// Makes every write of the transaction visible.
    pub async fn commit(self) -> AppResult<()> {
        match self.engine {
            TxEngine::Sqlite(tx) => tx.commit().await?,
            TxEngine::Mongo(mut mongo) => mongo.session.commit_transaction().await?,
        }
        Ok(())
    }
}
