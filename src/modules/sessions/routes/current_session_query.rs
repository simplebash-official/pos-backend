use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CurrentSessionQuery {
    pub terminal_id: Option<String>,
}
