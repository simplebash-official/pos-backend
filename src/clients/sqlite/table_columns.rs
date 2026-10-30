use sqlx::SqlitePool;

/// Column names of `table` (empty when the table does not exist).
pub(super) async fn table_columns(
    pool: &SqlitePool,
    table: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .fetch_all(pool)
        .await
}
