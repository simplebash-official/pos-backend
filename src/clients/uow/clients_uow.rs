use super::super::db::Db;
use super::super::tenant_db::TenantDatabase;
use crate::core::error::AppResult;
use mongodb::ClientSession;
use sqlx::pool::PoolConnection;
use sqlx::{Sqlite, SqliteConnection, Transaction};
use std::ops::{Deref, DerefMut};

/// What a repository needs to run its query on the engine behind a `Unit`.
#[derive(Debug)]
pub enum Backend<'a> {
    /// The tenant-aware handle, and the transaction's session when there is one.
    Mongo(&'a TenantDatabase, Option<&'a mut ClientSession>),
    /// Run statements with `.execute(&mut *conn)`.
    Sqlite(SqliteConn<'a>),
}

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

#[derive(Debug)]
pub(super) struct MongoTx {
    pub(super) db: TenantDatabase,
    pub(super) session: ClientSession,
}

/// A SQLite connection: a pooled one for a plain `Db`, the transaction's own for a `Tx`.
#[derive(Debug)]
pub enum SqliteConn<'a> {
    Pooled(PoolConnection<Sqlite>),
    Tx(&'a mut SqliteConnection),
}

impl Deref for SqliteConn<'_> {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        match self {
            SqliteConn::Pooled(conn) => conn,
            SqliteConn::Tx(conn) => conn,
        }
    }
}

impl DerefMut for SqliteConn<'_> {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        match self {
            SqliteConn::Pooled(conn) => conn,
            SqliteConn::Tx(conn) => conn,
        }
    }
}

/// An open transaction. Created by `Db::begin`.
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

#[derive(Debug)]
pub(super) enum TxEngine {
    Sqlite(Transaction<'static, Sqlite>),
    // Boxed: a driver session is far larger than a SQLite transaction handle.
    Mongo(Box<MongoTx>),
}

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
    /// The transaction backend, starting the transaction on first use.
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
