use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::quotations::model::QuotationDocument;
use crate::modules::quotations::repository;

pub(super) async fn find_document(db: &Db, id_or_key: &str) -> AppResult<QuotationDocument> {
    repository::find_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Quotation not found", codes::QUOTATION_NOT_FOUND)
        })
}
