/// Database engine type selected via environment configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    Mongo,
    Sqlite,
}
