use super::find_document::find_document;
use super::today::today;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::quotations::Quotation;

pub async fn get_quotation(db: &Db, id_or_key: &str) -> AppResult<Quotation> {
    Ok(find_document(db, id_or_key).await?.into_quotation(&today()))
}
