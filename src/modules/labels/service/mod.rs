// Label designs and label printing. A design is a shop's own layout (or one of
// the built-in starters); printing resolves the chosen products/serials into
// stickers server-side (barcode, name, SKU, category and price all come from
// the records, never from the client) and renders them through document-server
// as one PDF page per sticker. This module is the only caller of
// `documents::service::render_uncached` for labels.

pub(crate) mod jobs;
pub(crate) mod payload;
pub(crate) mod starters;
pub(crate) mod validate;

use axum::http::StatusCode;
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::{db::Db, document_server::DocumentServerClient},
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::labels::{
        LabelElement, LabelItem, LabelSample, LabelTemplate, LabelTemplateInput,
        LabelTemplatesResponse, PreviewLabelsRequest, RenderLabelsRequest, TextSource,
    },
    modules::{
        documents, inventory,
        labels::{model::LabelTemplateDocument, repository},
        settings,
    },
};
use payload::Sticker;

/// Call-site slug of the label template in `clients::document_server`.
const LABEL_SLUG: &str = "product-label";
/// Most distinct products/serials in one print.
const MAX_ITEMS: usize = 200;
/// Most copies of one item, and most stickers in one print (a page each).
const MAX_COPIES: u32 = 500;
const MAX_LABELS_PER_JOB: u64 = 500;

const TEMPLATE_NOT_FOUND: &str = "LABEL_TEMPLATE_NOT_FOUND";
const TEMPLATE_NAME_TAKEN: &str = "LABEL_TEMPLATE_NAME_TAKEN";
const TEMPLATE_BUILT_IN: &str = "LABEL_TEMPLATE_BUILT_IN";
const PRODUCT_WITHOUT_BARCODE: &str = "LABEL_PRODUCT_WITHOUT_BARCODE";
const PRICE_REQUIRED: &str = "LABEL_PRICE_REQUIRED";

fn template_not_found() -> AppError {
    AppError::not_found_with_code("Label design not found", TEMPLATE_NOT_FOUND)
}

fn built_in_conflict() -> AppError {
    AppError::custom(
        StatusCode::CONFLICT,
        TEMPLATE_BUILT_IN,
        "A ready-made design cannot be changed. Copy it to make your own.",
    )
}

/// The starters, then the shop's own designs by name.
pub(crate) async fn list_templates(db: &Db) -> AppResult<LabelTemplatesResponse> {
    let mut templates = starters::all();
    templates.extend(
        repository::list_templates(db)
            .await?
            .into_iter()
            .map(LabelTemplateDocument::into_template),
    );
    Ok(LabelTemplatesResponse { templates })
}

/// A design by key: a starter (`starter_*`) or one the shop built (`ltpl_*`).
async fn find_template(db: &Db, key: &str) -> AppResult<LabelTemplate> {
    if key.starts_with(starters::STARTER_KEY_PREFIX) {
        return starters::find(key).ok_or_else(template_not_found);
    }
    repository::find_template_by_key(db, key)
        .await?
        .map(LabelTemplateDocument::into_template)
        .ok_or_else(template_not_found)
}

/// Design names are unique per shop (starters included), ignoring case, so the
/// template picker never shows two identical entries.
async fn ensure_name_free(db: &Db, name: &str, own_key: Option<&str>) -> AppResult<()> {
    let wanted = name.trim().to_lowercase();
    let taken = list_templates(db)
        .await?
        .templates
        .iter()
        .any(|t| Some(t.key.as_str()) != own_key && t.name.trim().to_lowercase() == wanted);
    if taken {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            TEMPLATE_NAME_TAKEN,
            format!("A label design called '{}' already exists", name.trim()),
        ));
    }
    Ok(())
}

/// Saves a new shop design.
pub(crate) async fn create_template(
    db: &Db,
    input: LabelTemplateInput,
) -> AppResult<LabelTemplate> {
    crate::core::logging::domain::tracked("labels.template_created", async move {
        validate::validate_template(&input)?;
        ensure_name_free(db, &input.name, None).await?;
        let now = BsonDateTime::now();
        let document = LabelTemplateDocument {
            id: None,
            key: generate_id(prefixes::LABEL_TEMPLATE),
            name: input.name.trim().to_string(),
            width_mm: input.width_mm,
            height_mm: input.height_mm,
            elements: input.elements,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        repository::insert_template(db, &document).await?;
        Ok(document.into_template())
    })
    .await
}

/// Replaces a shop design. A starter is refused (409 `LABEL_TEMPLATE_BUILT_IN`).
pub(crate) async fn update_template(
    db: &Db,
    key: &str,
    input: LabelTemplateInput,
) -> AppResult<LabelTemplate> {
    crate::core::logging::domain::tracked("labels.template_updated", async move {
        if key.starts_with(starters::STARTER_KEY_PREFIX) {
            return Err(built_in_conflict());
        }
        validate::validate_template(&input)?;
        ensure_name_free(db, &input.name, Some(key)).await?;
        repository::update_template(
            db,
            key,
            input.name.trim(),
            input.width_mm,
            input.height_mm,
            &input.elements,
        )
        .await?
        .map(LabelTemplateDocument::into_template)
        .ok_or_else(template_not_found)
    })
    .await
}

/// Deletes a shop design. A starter is refused (409 `LABEL_TEMPLATE_BUILT_IN`).
pub(crate) async fn delete_template(db: &Db, key: &str) -> AppResult<()> {
    crate::core::logging::domain::tracked("labels.template_deleted", async move {
        if key.starts_with(starters::STARTER_KEY_PREFIX) {
            return Err(built_in_conflict());
        }
        if repository::delete_template(db, key).await? {
            Ok(())
        } else {
            Err(template_not_found())
        }
    })
    .await
}

fn has_price_element(elements: &[LabelElement]) -> bool {
    elements.iter().any(|element| {
        matches!(
            element,
            LabelElement::Text {
                source: TextSource::Price,
                ..
            }
        )
    })
}

fn check_copies(copies: u32) -> AppResult<()> {
    if (1..=MAX_COPIES).contains(&copies) {
        Ok(())
    } else {
        Err(AppError::validation(format!(
            "Copies must be between 1 and {MAX_COPIES}."
        )))
    }
}

/// Turns the requested items into stickers using the stored records. Fails
/// before anything is rendered when an item cannot be printed.
async fn resolve_stickers(
    db: &Db,
    needs_price: bool,
    items: &[LabelItem],
) -> AppResult<Vec<Sticker>> {
    if items.is_empty() {
        return Err(AppError::validation("Choose at least one item to print."));
    }
    if items.len() > MAX_ITEMS {
        return Err(AppError::validation(format!(
            "You can print labels for at most {MAX_ITEMS} items at a time."
        )));
    }

    let mut total: u64 = 0;
    let mut stickers = Vec::with_capacity(items.len());
    for item in items {
        let sticker = match item {
            LabelItem::Product {
                product_key,
                copies,
                price_cents,
            } => {
                check_copies(*copies)?;
                let product =
                    inventory::service::product::get_product_by_key(db, product_key).await?;
                let code = product.barcode.clone().ok_or_else(|| {
                    AppError::unprocessable_entity(
                        PRODUCT_WITHOUT_BARCODE,
                        format!("'{}' has no barcode yet. Create one first.", product.name),
                    )
                })?;
                let price = match price_cents {
                    Some(price) => {
                        if !product
                            .price_tiers
                            .iter()
                            .any(|tier| tier.selling_price_cents == *price)
                        {
                            return Err(AppError::unprocessable_entity(
                                codes::PRICE_TIER_UNAVAILABLE,
                                format!("'{}' is not in stock at that price.", product.name),
                            ));
                        }
                        Some(*price)
                    }
                    None if needs_price => {
                        return Err(AppError::unprocessable_entity(
                            PRICE_REQUIRED,
                            format!("Choose a price for '{}'.", product.name),
                        ));
                    }
                    None => None,
                };
                Sticker {
                    product_key: product.key,
                    serial_number: None,
                    name: product.name,
                    sku: product.sku,
                    category: product.category,
                    price_cents: price,
                    code,
                    copies: *copies,
                }
            }
            LabelItem::Serial {
                serial_number,
                copies,
            } => {
                check_copies(*copies)?;
                let lookup =
                    inventory::service::product_serial::lookup_in_stock_serial(db, serial_number)
                        .await?;
                Sticker {
                    product_key: lookup.product.key,
                    serial_number: Some(lookup.serial.serial_number.clone()),
                    name: lookup.product.name,
                    sku: lookup.product.sku,
                    category: lookup.product.category,
                    price_cents: Some(lookup.selling_price_cents),
                    code: lookup.serial.serial_number,
                    copies: *copies,
                }
            }
        };
        total += u64::from(sticker.copies);
        stickers.push(sticker);
    }
    if total > MAX_LABELS_PER_JOB {
        return Err(AppError::validation(format!(
            "You can print at most {MAX_LABELS_PER_JOB} labels at a time ({total} requested)."
        )));
    }
    Ok(stickers)
}

/// Draws `stickers` with the given layout through document-server.
async fn render(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    width_mm: f64,
    height_mm: f64,
    elements: &[LabelElement],
    stickers: &[Sticker],
) -> AppResult<Vec<u8>> {
    for sticker in stickers {
        validate::check_code_fits(elements, &sticker.code, &sticker.name)?;
    }
    let shop = settings::service::get_shop_profile(db, tenant_id).await?;
    let data =
        payload::build_label_data(width_mm, height_mm, &shop.trading_name, elements, stickers)?;
    documents::service::render_uncached(doc_server, LABEL_SLUG, data).await
}

/// Prints the chosen items with a saved design or starter: one PDF page per sticker.
pub(crate) async fn render_labels(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    request: RenderLabelsRequest,
) -> AppResult<Vec<u8>> {
    let template = find_template(db, &request.template_key).await?;
    let stickers =
        resolve_stickers(db, has_price_element(&template.elements), &request.items).await?;
    render(
        db,
        doc_server,
        tenant_id,
        template.width_mm,
        template.height_mm,
        &template.elements,
        &stickers,
    )
    .await
}

/// Draws one sticker of an unsaved design with example values, for the
/// designer's live preview. Applies the same rules as a real print.
pub(crate) async fn preview_label(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    request: PreviewLabelsRequest,
) -> AppResult<Vec<u8>> {
    validate::validate_template(&request.template)?;
    let LabelSample {
        name,
        sku,
        category,
        price_cents,
        code,
    } = request.sample;
    if code.is_empty() || code.chars().count() > 64 {
        return Err(AppError::validation(
            "The sample code must be 1 to 64 characters.",
        ));
    }
    // A preview is a sample, never recorded, so it belongs to no product.
    let sticker = Sticker {
        product_key: String::new(),
        serial_number: None,
        name,
        sku,
        category,
        price_cents: Some(price_cents),
        code,
        copies: 1,
    };
    render(
        db,
        doc_server,
        tenant_id,
        request.template.width_mm,
        request.template.height_mm,
        &request.template.elements,
        &[sticker],
    )
    .await
}
