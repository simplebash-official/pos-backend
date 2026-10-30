use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SetupStatusQuery {
    pub shop: Option<String>,
}
