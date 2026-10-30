use super::suppliers::suppliers;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

pub(crate) async fn count_stats(db: &Db) -> AppResult<(u64, u64)> {
    match db {
        Db::Mongo(db) => {
            let total_suppliers = suppliers(db)
                .count_documents(doc! { "deleted_at": { "$exists": false } })
                .await?;
            let direct_contacts_count = suppliers(db)
                .count_documents(doc! {
                    "deleted_at": { "$exists": false },
                    "contact_person": { "$ne": "" }
                })
                .await?;
            Ok((total_suppliers, direct_contacts_count))
        }
        Db::Sqlite(pool) => {
            #[derive(Debug, sqlx::FromRow)]
            struct SupplierCounts {
                total: i64,
                direct: i64,
            }

            let counts = sqlx::query_as::<_, SupplierCounts>(
                r#"
                SELECT
                    COUNT(*) as total,
                    COUNT(CASE WHEN contact_person IS NOT NULL AND contact_person != '' THEN 1 ELSE NULL END) as direct
                FROM suppliers
                WHERE deleted_at IS NULL
                "#,
            )
            .fetch_one(pool)
            .await?;

            Ok((counts.total as u64, counts.direct as u64))
        }
    }
}
