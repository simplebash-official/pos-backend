use super::payload::Sticker;
use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;

use crate::core::error::AppResult;

use crate::domain::labels::LabelElement;

use crate::modules::{documents, settings};

/// Call-site slug of the label template in `clients::document_server`.
const LABEL_SLUG: &str = "product-label";

/// Draws `stickers` with the given layout through document-server.
pub(super) async fn render(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    width_mm: f64,
    height_mm: f64,
    elements: &[LabelElement],
    stickers: &[Sticker],
) -> AppResult<Vec<u8>> {
    for sticker in stickers {
        super::validate::check_code_fits(elements, &sticker.code, &sticker.name)?;
    }
    let shop = settings::service::get_shop_profile(db, tenant_id).await?;
    let data = super::payload::build_label_data(
        width_mm,
        height_mm,
        &shop.trading_name,
        elements,
        stickers,
    )?;
    documents::service::render_uncached(doc_server, LABEL_SLUG, data).await
}
