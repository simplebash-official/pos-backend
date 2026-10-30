use super::recoveries::recoveries;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::AppResult;
use crate::modules::balances::model::AdvanceRecoveryDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

fn recovery_from_row(row: &sqlx::sqlite::SqliteRow) -> AdvanceRecoveryDocument {
    AdvanceRecoveryDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        employee_key: row.get("employee_key"),
        amount_cents: row.get("amount_cents"),
        recovered_on: row.get("recovered_on"),
        note: row.get("note"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    }
}

/// Every recovery, oldest first; the service groups them per employee.
pub(crate) async fn list_recoveries(db: &Db) -> AppResult<Vec<AdvanceRecoveryDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = recoveries(db)
                .find(doc! {})
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM advance_recoveries ORDER BY created_at ASC")
                .fetch_all(pool)
                .await?;
            Ok(rows.iter().map(recovery_from_row).collect())
        }
    }
}
