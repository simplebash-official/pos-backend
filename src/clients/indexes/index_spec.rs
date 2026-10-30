use mongodb::bson::Document;

use std::time::Duration;

/// One index to ensure: enough to build an `IndexModel` and to assert on in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexSpec {
    pub collection: &'static str,
    pub name: String,
    pub keys: Document,
    pub unique: bool,
    pub ttl: Option<Duration>,
}
