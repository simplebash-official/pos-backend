// Unit of work: one database transaction that spans several repository calls,
// on either engine. `Db::begin` opens it, repositories write through it, and
// `Tx::commit` makes every write visible at once. Dropping a `Tx` without
// committing rolls everything back (sqlx rolls the transaction back on drop,
// and the Mongo driver aborts an open transaction when its session drops).
//
// A repository function takes `impl Into<Unit<'_>>`, so it accepts a plain
// `&Db` (every statement commits on its own, as before) or a `&mut Tx` (the
// statement joins the transaction). There is one implementation of each query,
// not an auto-commit one and a transactional one.
//
// Inside a transaction, every read and write on the path must go through the
// same `Tx`: on SQLite a second connection would wait on the write lock the
// transaction already holds, and on Mongo a read outside the session cannot see
// the session's uncommitted writes.

use std::ops::{Deref, DerefMut};

use mongodb::ClientSession;
use sqlx::{Sqlite, SqliteConnection, Transaction, pool::PoolConnection};

use super::{db::Db, tenant_db::TenantDatabase};
use crate::core::error::AppResult;

/// An open transaction. Created by [`Db::begin`].
pub struct Tx {
    engine: TxEngine,
}

enum TxEngine {
    Sqlite(Transaction<'static, Sqlite>),
    // Boxed: a driver session is far larger than a SQLite transaction handle.
    Mongo(Box<MongoTx>),
}

struct MongoTx {
    db: TenantDatabase,
    session: ClientSession,
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

/// Where a repository call runs: straight on the database, or inside a `Tx`.
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

/// A SQLite connection: a pooled one for a plain `Db`, the transaction's own for a `Tx`.
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

/// What a repository needs to run its query on the engine behind a [`Unit`].
pub enum Backend<'a> {
    /// The tenant-aware handle, and the transaction's session when there is one.
    Mongo(&'a TenantDatabase, Option<&'a mut ClientSession>),
    /// Run statements with `.execute(&mut *conn)`.
    Sqlite(SqliteConn<'a>),
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

/// Runs a Mongo action inside the transaction's session when there is one.
/// `$action` is the driver builder (`find_one(..)`, `update_one(..)`, ...) and
/// `$session` the `Option<&mut ClientSession>` from [`Backend::Mongo`].
#[macro_export]
macro_rules! in_session {
    ($action:expr, $session:expr) => {
        match $session {
            Some(session) => $action.session(session).await,
            None => $action.await,
        }
    };
}
