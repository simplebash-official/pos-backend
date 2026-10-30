use super::sessions_col::sessions_col;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn increment_session_refunds(
    db: &Db,
    session_key: &str,
    refund_cash_delta: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    match db {
        Db::Mongo(mongo_db) => {
            sessions_col(mongo_db)
                .update_one(
                    doc! { "key": session_key },
                    doc! {
                        "$inc": {
                            "total_cash_refunds_cents": refund_cash_delta,
                            "version": 1,
                        },
                        "$set": { "updated_at": now },
                    },
                )
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let updated_at_iso = bson_to_iso(&now);
            sqlx::query(
                r#"
                UPDATE register_sessions SET
                    total_cash_refunds_cents = total_cash_refunds_cents + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ?
                "#,
            )
            .bind(refund_cash_delta)
            .bind(&updated_at_iso)
            .bind(session_key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
