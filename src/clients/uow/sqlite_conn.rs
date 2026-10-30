use sqlx::pool::PoolConnection;
use sqlx::{Sqlite, SqliteConnection};
use std::ops::{Deref, DerefMut};

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
