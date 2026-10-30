use super::super::SqlLogging;

/// The logging switches in force after a control line was applied.
#[derive(Debug, Clone, Copy)]
pub struct LogMode {
    pub enabled: bool,
    pub http_bodies: bool,
    pub sql: SqlLogging,
}
