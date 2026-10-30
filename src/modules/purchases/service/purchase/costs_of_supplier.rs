use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::modules::purchases::{charges, repository};

/// What was bought from a supplier, as `(YYYY-MM-DD, cents)` per entry: each purchase line's
/// price, and each charge the supplier put on a bill (a discount is negative). The charges a
/// receipt paid to someone else are not owed to the supplier and are left out. This and the
/// receipt total share `charges::owed_total`'s definition, so the balance and the payables agree.
pub(crate) async fn costs_of_supplier(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<(String, i64)>> {
    let day = |date: mongodb::bson::DateTime| date.to_chrono().format("%Y-%m-%d").to_string();
    let mut costs: Vec<(String, i64)> = repository::list_purchases_by_supplier(db, supplier_key)
        .await?
        .into_iter()
        .map(|p| (day(p.date), p.line_total_cents))
        .collect();
    costs.extend(
        repository::charge::list_supplier_billed_charges(db, supplier_key)
            .await?
            .into_iter()
            .map(|c| (day(c.date), charges::signed_amount(c.kind, c.amount_cents))),
    );
    Ok(costs)
}
