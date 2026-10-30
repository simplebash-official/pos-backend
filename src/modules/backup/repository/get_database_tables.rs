use super::is_excluded::is_excluded;
use sqlx::{Row, SqlitePool};

/// Discovers all persistent user/application tables in the SQLite database dynamically.
pub(crate) async fn get_database_tables(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' ORDER BY name;",
    )
    .fetch_all(pool)
    .await?;

    let tables: Vec<String> = rows
        .into_iter()
        .map(|r| r.get::<String, _>(0))
        .filter(|name| !is_excluded(name))
        .collect();

    Ok(tables)
}
