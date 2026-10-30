use super::sessions_col::sessions_col;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn increment_session_sales(
    db: &Db,
    session_key: &str,
    cash_delta: i64,
    card_delta: i64,
    online_delta: i64,
    credit_delta: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    match db {
        Db::Mongo(mongo_db) => {
            sessions_col(mongo_db)
                .update_one(
                    doc! { "key": session_key },
                    doc! {
                        "$inc": {
                            "total_cash_sales_cents": cash_delta,
                            "total_card_sales_cents": card_delta,
                            "total_online_sales_cents": online_delta,
                            "total_credit_sales_cents": credit_delta,
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
                    total_cash_sales_cents = total_cash_sales_cents + ?,
                    total_card_sales_cents = total_card_sales_cents + ?,
                    total_online_sales_cents = total_online_sales_cents + ?,
                    total_credit_sales_cents = total_credit_sales_cents + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ?
                "#,
            )
            .bind(cash_delta)
            .bind(card_delta)
            .bind(online_delta)
            .bind(credit_delta)
            .bind(&updated_at_iso)
            .bind(session_key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
