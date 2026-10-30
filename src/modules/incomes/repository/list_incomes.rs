use super::income_from_row::income_from_row;
use super::incomes::incomes;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::incomes::model::IncomeDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every income dated between two days (`YYYY-MM-DD`, inclusive; `None` is open-ended).
pub(crate) async fn list_incomes(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<IncomeDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut range = doc! {};
            if let Some(from) = from {
                range.insert("$gte", from);
            }
            if let Some(to) = to {
                range.insert("$lte", to);
            }
            let filter = if range.is_empty() {
                doc! {}
            } else {
                doc! { "income_date": range }
            };
            let mut cursor = incomes(db).find(filter).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut sql = String::from("SELECT * FROM incomes WHERE 1 = 1");
            if from.is_some() {
                sql.push_str(" AND income_date >= ?");
            }
            if to.is_some() {
                sql.push_str(" AND income_date <= ?");
            }
            let mut query = sqlx::query(&sql);
            if let Some(from) = from {
                query = query.bind(from);
            }
            if let Some(to) = to {
                query = query.bind(to);
            }
            query
                .fetch_all(pool)
                .await?
                .iter()
                .map(income_from_row)
                .collect()
        }
    }
}
