use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ControlLine {
    pub cmd: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub http_bodies: bool,
    #[serde(default)]
    pub sql: Option<String>,
}

fn default_true() -> bool {
    true
}
