use super::cash_movements_col::cash_movements_col;
use super::movement_from_sqlite_row::movement_from_sqlite_row;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};
use crate::domain::sessions::CashMovementType;
use crate::modules::sessions::model::CashMovementDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Movements of the given types recorded on or between two days (`YYYY-MM-DD`, UTC, inclusive;
/// `None` is open-ended), oldest first. For the expense ledger, which reads drawer payments
/// straight from here.
pub async fn list_movements_of_types(
    db: &Db,
    types: &[CashMovementType],
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<CashMovementDocument>> {
    let type_names: Vec<&str> = types.iter().map(CashMovementType::as_str).collect();
    match db {
        Db::Mongo(mongo_db) => {
            let mut filter = doc! { "movement_type": { "$in": &type_names } };
            let mut range = doc! {};
            let day_start = |day: &str| -> AppResult<mongodb::bson::DateTime> {
                let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
                    .map_err(|_| AppError::validation("Please pick a valid date."))?;
                Ok(mongodb::bson::DateTime::from_chrono(
                    date.and_hms_opt(0, 0, 0)
                        .expect("midnight exists")
                        .and_utc(),
                ))
            };
            if let Some(from) = from {
                range.insert("$gte", day_start(from)?);
            }
            if let Some(to) = to {
                // Inclusive end day: everything before the next midnight.
                let next = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
                    .map_err(|_| AppError::validation("Please pick a valid date."))?
                    .succ_opt()
                    .ok_or_else(|| AppError::validation("Please pick a valid date."))?;
                range.insert("$lt", day_start(&next.format("%Y-%m-%d").to_string())?);
            }
            if !range.is_empty() {
                filter.insert("created_at", range);
            }
            let mut cursor = cash_movements_col(mongo_db)
                .find(filter)
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                out.push(item);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let placeholders = type_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let mut sql = format!(
                "SELECT * FROM cash_movements WHERE deleted_at IS NULL AND movement_type IN ({placeholders})"
            );
            if from.is_some() {
                sql.push_str(" AND substr(created_at, 1, 10) >= ?");
            }
            if to.is_some() {
                sql.push_str(" AND substr(created_at, 1, 10) <= ?");
            }
            sql.push_str(" ORDER BY created_at ASC");
            let mut query = sqlx::query(&sql);
            for name in &type_names {
                query = query.bind(*name);
            }
            if let Some(from) = from {
                query = query.bind(from);
            }
            if let Some(to) = to {
                query = query.bind(to);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(movement_from_sqlite_row).collect()
        }
    }
}
