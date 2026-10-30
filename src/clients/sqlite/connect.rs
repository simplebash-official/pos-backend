use super::init_db::init_db;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use sqlx::SqlitePool;
use std::path::Path;

/// Initializes the SQLite connection pool, creates parent directories if needed,
/// applies optimal concurrency PRAGMAs, and runs initial schema migrations.
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    // If the URL points to a file, ensure its parent directory exists
    if let Some(path_str) = url.strip_prefix("sqlite://") {
        let clean_path = path_str.split('?').next().unwrap_or(path_str);
        if clean_path != ":memory:" && !clean_path.is_empty() {
            let path = Path::new(clean_path);
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                let _ = std::fs::create_dir_all(parent);
            }
        }
    }

    let options: SqliteConnectOptions = url.parse()?;
    let options = with_statement_logging(options)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(options)
        .await?;

    init_db(&pool).await?;

    Ok(pool)
}

/// Wires sqlx statement logging unless the process started with
/// `LOG_SQL=off`. Which statements actually reach the log is then decided per
/// event by the runtime filter in `core::logging::init`, so a `log_mode`
/// control line can switch between every statement, slow-only and none
/// without reconnecting the pool.
fn with_statement_logging(options: SqliteConnectOptions) -> SqliteConnectOptions {
    use crate::core::logging::{SqlLogging, settings};
    use sqlx::ConnectOptions;
    if settings().sql_at_startup == SqlLogging::Off {
        return options.disable_statement_logging();
    }
    options
        .log_statements(log::LevelFilter::Info)
        .log_slow_statements(
            log::LevelFilter::Warn,
            std::time::Duration::from_millis(250),
        )
}
