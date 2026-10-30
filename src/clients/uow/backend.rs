use super::super::tenant_db::TenantDatabase;
use super::sqlite_conn::SqliteConn;

use mongodb::ClientSession;

/// What a repository needs to run its query on the engine behind a [`Unit`].
#[derive(Debug)]
pub enum Backend<'a> {
    /// The tenant-aware handle, and the transaction's session when there is one.
    Mongo(&'a TenantDatabase, Option<&'a mut ClientSession>),
    /// Run statements with `.execute(&mut *conn)`.
    Sqlite(SqliteConn<'a>),
}
