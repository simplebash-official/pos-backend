use super::opening_from_row::opening_from_row;
use super::openings::openings;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::balances::BalanceSubject;
use crate::modules::balances::model::BalanceOpeningDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every opening of one kind (suppliers or staff).
pub(crate) async fn list_openings(
    db: &Db,
    kind: BalanceSubject,
) -> AppResult<Vec<BalanceOpeningDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = openings(db)
                .find(doc! { "subject_kind": kind.as_str() })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM balance_openings WHERE subject_kind = ?")
                .bind(kind.as_str())
                .fetch_all(pool)
                .await?;
            rows.iter().map(opening_from_row).collect()
        }
    }
}
