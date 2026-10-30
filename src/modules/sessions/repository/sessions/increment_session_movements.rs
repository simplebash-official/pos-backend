use super::sessions_col::sessions_col;

use crate::clients::sqlite::bson_to_iso;

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::domain::sessions::CashMovementType;

use mongodb::bson::{DateTime as BsonDateTime, doc};

pub async fn increment_session_movements(
    unit: impl Into<Unit<'_>>,
    session_key: &str,
    movement_type: CashMovementType,
    amount_cents: i64,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    let col_name = match movement_type {
        CashMovementType::AddCash => "total_pay_in_cents",
        CashMovementType::PettyExpense
        | CashMovementType::RentAndBills
        | CashMovementType::StaffPayment
        | CashMovementType::SupplierPayment => "total_pay_out_cents",
        CashMovementType::OwnerWithdrawal => "total_owner_withdrawals_cents",
        CashMovementType::CashDrop => "total_cash_drops_cents",
        CashMovementType::NoSale => return Ok(()),
    };

    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(mongo_db, session) => {
            crate::in_session!(
                sessions_col(mongo_db).update_one(
                    doc! { "key": session_key },
                    doc! {
                        "$inc": { col_name: amount_cents, "version": 1 },
                        "$set": { "updated_at": now },
                    },
                ),
                session
            )?;
            Ok(())
        }
        Backend::Sqlite(mut conn) => {
            let updated_at_iso = bson_to_iso(&now);
            let sql = format!(
                r#"
                UPDATE register_sessions SET
                    {} = {} + ?,
                    version = version + 1,
                    updated_at = ?
                WHERE key = ?
                "#,
                col_name, col_name
            );
            sqlx::query(&sql)
                .bind(amount_cents)
                .bind(&updated_at_iso)
                .bind(session_key)
                .execute(&mut *conn)
                .await?;
            Ok(())
        }
    }
}
