//! Test-harness pieces: test app.

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
