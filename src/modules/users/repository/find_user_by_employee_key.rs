use super::user_from_sqlite_row::user_from_sqlite_row;
use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;

use mongodb::bson::{Bson, doc};

/// Looked up by linked employee key — backs `service::find_user_by_employee_key`,
/// the entry point `modules::employees` uses to answer "does this employee
/// already have a login" (on create-validation and on delete's
/// `EMPLOYEE_HAS_LOGIN` guard).
pub(crate) async fn find_user_by_employee_key(
    db: &Db,
    employee_key: &str,
) -> AppResult<Option<UserDocument>> {
    match db {
        Db::Mongo(db) => Ok(users(db)
            .find_one(doc! { "employee_key": employee_key, "deleted_at": Bson::Null })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM users WHERE employee_key = $1")
                .bind(employee_key)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(user_from_sqlite_row))
        }
    }
}
