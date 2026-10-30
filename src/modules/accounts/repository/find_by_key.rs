use super::openings::openings;
use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::accounts::AccountKind;
use crate::modules::accounts::model::AccountOpeningDocument;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<AccountOpeningDocument> {
    let kind: String = row.get("kind");
    Ok(AccountOpeningDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        kind: AccountKind::parse(&kind)
            .ok_or_else(|| AppError::internal(format!("Unknown account '{kind}' in database")))?,
        opening_balance_cents: row.get("opening_balance_cents"),
        as_of_date: row.get("as_of_date"),
        set_by_name_snapshot: row.get("set_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// The opening balance row of one account, if it has been set.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<AccountOpeningDocument>> {
    match db {
        Db::Mongo(db) => Ok(openings(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM account_openings WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}
