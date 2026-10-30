use super::find_document::find_document;
use super::today::today;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::quotations::Quotation;

/// The quotation plus its frozen shop profile, for the PDF builder only.
pub(crate) async fn get_quotation_for_print(
    db: &Db,
    id_or_key: &str,
) -> AppResult<(Quotation, serde_json::Value)> {
    let document = find_document(db, id_or_key).await?;
    let shop = document.shop_profile_snapshot.clone();
    Ok((document.into_quotation(&today()), shop))
}
