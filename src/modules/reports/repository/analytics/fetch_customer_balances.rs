use super::super::sales::get_i64_flexible;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};

/// `customer.key -> outstanding_balance_cents` for the given keys.
pub(crate) async fn fetch_customer_balances(
    db: &Db,
    keys: &[String],
) -> AppResult<std::collections::HashMap<String, i64>> {
    match db {
        Db::Mongo(db) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            let mut cursor = db
                .collection::<Document>("customers")
                .find(doc! { "key": { "$in": keys } })
                .projection(doc! { "key": 1, "outstanding_balance_cents": 1 })
                .await?;
            while let Some(row) = cursor.try_next().await? {
                if let Ok(k) = row.get_str("key") {
                    out.insert(
                        k.to_string(),
                        get_i64_flexible(&row, "outstanding_balance_cents"),
                    );
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            use sqlx::Row;
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT key, outstanding_balance_cents FROM customers WHERE key IN ({placeholders})"
            );
            let mut q = sqlx::query(&sql);
            for k in keys {
                q = q.bind(k);
            }
            let rows = q.fetch_all(pool).await?;
            for r in rows {
                let k: String = r.get("key");
                let bal: i64 = r.try_get("outstanding_balance_cents").unwrap_or(0);
                out.insert(k, bal);
            }
            Ok(out)
        }
    }
}
