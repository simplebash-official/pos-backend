//! Test-harness pieces: test app.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use mongodb::Database;
use simplebash_pos_backend::core::config::Config;

pub struct TestApp {
    #[allow(dead_code)]
    pub router: Router,
    // Not every integration test binary that includes this module needs
    // direct DB access (e.g. to seed data outside the API) or the config
    // (e.g. to mint a JWT with `jwt_secret`) — these fields are dead code
    // from the perspective of whichever one doesn't.
    /// The raw MongoDB handle; `None` for apps running on SQLite.
    pub(super) db: Option<Database>,
    #[allow(dead_code)]
    pub db_handle: simplebash_pos_backend::clients::db::Db,
    #[allow(dead_code)]
    pub config: Arc<Config>,
    /// Removes the SQLite database when the app is dropped; `None` for apps on MongoDB. Kept
    /// last, so the handles above are dropped first.
    pub(super) _sqlite_file: Option<SqliteDatabaseFile>,
}

/// A SQLite test database that is removed from disk when dropped.
pub(super) struct SqliteDatabaseFile(pub(super) PathBuf);

impl Drop for SqliteDatabaseFile {
    fn drop(&mut self) {
        remove_sqlite_files(&self.0);
    }
}

/// Removes a SQLite database with its `-wal` and `-shm` files. Best-effort test cleanup: a file
/// that was never created, or cannot be removed, does not fail the test.
#[allow(dead_code)]
pub fn remove_sqlite_files(path: &Path) {
    let sidecars = [path.with_extension("db-wal"), path.with_extension("db-shm")];
    let _ = fs::remove_file(path);
    for sidecar in sidecars {
        let _ = fs::remove_file(sidecar);
    }
}

impl TestApp {
    /// The raw MongoDB handle, for tests that seed or inspect data outside the API.
    #[allow(dead_code)]
    pub fn mongo(&self) -> &Database {
        self.db
            .as_ref()
            .expect("this test app runs on SQLite and has no MongoDB handle")
    }
}
