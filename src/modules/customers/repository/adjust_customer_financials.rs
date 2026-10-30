use super::customers::customers;
use super::find_customer_by_key::find_customer_by_key;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

/// Atomically adjusts customer total purchases and outstanding balance via `$inc`.
/// Used by billing/invoices and payments modules.
#[allow(dead_code)]
pub(crate) async fn adjust_customer_financials(
    db: &Db,
    key: &str,
    purchases_delta: i64,
    balance_delta: i64,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            Ok(customers(db)
                .find_one_and_update(
                    doc! { "key": key, "deleted_at": { "$exists": false } },
                    doc! {
                        "$inc": {
                            "total_purchases_cents": purchases_delta,
                            "outstanding_balance_cents": balance_delta,
                            "version": 1,
                        },
                        "$set": {
                            "updated_at": now,
                        }
                    },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let existing = find_customer_by_key(db, key).await?;
            let Some(mut cust) = existing else {
                return Ok(None);
            };

            cust.total_purchases_cents += purchases_delta;
            cust.outstanding_balance_cents += balance_delta;
            cust.version += 1;
            cust.updated_at = BsonDateTime::now();

            let updated_at_iso = bson_to_iso(&cust.updated_at);

            sqlx::query(
                r#"
                UPDATE customers SET
                    total_purchases_cents = total_purchases_cents + ?,
                    outstanding_balance_cents = outstanding_balance_cents + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ? AND deleted_at IS NULL
                "#,
            )
            .bind(purchases_delta)
            .bind(balance_delta)
            .bind(&updated_at_iso)
            .bind(key)
            .execute(pool)
            .await?;

            Ok(Some(cust))
        }
    }
}
