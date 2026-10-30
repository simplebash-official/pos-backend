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

mod backend;
mod impl_for_db;
mod macro_rules;
mod mongo_tx;
mod sqlite_conn;
mod tx;
mod tx_engine;
mod unit;

pub use backend::Backend;
pub use sqlite_conn::SqliteConn;
pub use tx::Tx;
pub use unit::Unit;
