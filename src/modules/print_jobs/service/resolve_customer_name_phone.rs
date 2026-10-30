use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::customers;

/// When `customer_key` resolves to a real customer, that record is
/// authoritative for name/phone — a client-typed name/phone alongside a
/// valid key could otherwise silently diverge from the linked customer.
/// Falls back to the client-supplied name/phone only when there's no key
/// (a walk-in with no linked account). Mirrors
/// `billing::service::sale::complete_sale`'s customer-key resolution.
pub(super) async fn resolve_customer_name_phone(
    db: &Db,
    customer_key: Option<&str>,
    fallback_name: String,
    fallback_phone: Option<String>,
) -> AppResult<(String, Option<String>)> {
    match customer_key {
        Some(key) => {
            let customer = customers::service::get_customer_by_key(db, key).await?;
            Ok((customer.name, Some(customer.primary_phone)))
        }
        None => Ok((fallback_name, fallback_phone)),
    }
}
