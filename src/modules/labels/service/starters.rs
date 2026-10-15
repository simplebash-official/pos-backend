// The ready-made label designs every shop starts with. They live in code, not in
// the database, so they are identical for every shop, always present and can
// never be edited or deleted; a shop copies one to make its own.

use crate::domain::labels::{LabelElement, LabelTemplate, TextAlign, TextSource};

/// Every starter key begins with this; shop templates use the `ltpl_` prefix.
pub(crate) const STARTER_KEY_PREFIX: &str = "starter_";

#[allow(clippy::too_many_arguments)]
fn text(
    id: &str,
    source: TextSource,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    font_size_pt: f64,
    bold: bool,
    align: TextAlign,
) -> LabelElement {
    LabelElement::Text {
        id: id.to_string(),
        source,
        custom_text: None,
        x_mm: x,
        y_mm: y,
        width_mm: width,
        height_mm: height,
        font_size_pt,
        bold,
        align,
    }
}

fn starter(
    key: &str,
    name: &str,
    width: f64,
    height: f64,
    elements: Vec<LabelElement>,
) -> LabelTemplate {
    LabelTemplate {
        key: format!("{STARTER_KEY_PREFIX}{key}"),
        name: name.to_string(),
        width_mm: width,
        height_mm: height,
        elements,
        built_in: true,
        created_at: None,
        updated_at: None,
    }
}

/// The starters, in the order they are listed.
pub(crate) fn all() -> Vec<LabelTemplate> {
    vec![
        starter(
            "price_tag_50x30",
            "Price tag 50 x 30 mm",
            50.0,
            30.0,
            vec![
                text(
                    "name",
                    TextSource::ProductName,
                    2.0,
                    1.5,
                    46.0,
                    6.0,
                    8.0,
                    true,
                    TextAlign::Left,
                ),
                LabelElement::Barcode {
                    id: "barcode".into(),
                    x_mm: 2.0,
                    y_mm: 8.0,
                    width_mm: 46.0,
                    height_mm: 15.0,
                    show_value: true,
                    font_size_pt: 6.0,
                },
                text(
                    "price",
                    TextSource::Price,
                    2.0,
                    24.0,
                    46.0,
                    5.0,
                    9.0,
                    true,
                    TextAlign::Right,
                ),
            ],
        ),
        starter(
            "shelf_58x40",
            "Shelf label 58 x 40 mm",
            58.0,
            40.0,
            vec![
                LabelElement::Qr {
                    id: "qr".into(),
                    x_mm: 2.0,
                    y_mm: 2.0,
                    width_mm: 22.0,
                    height_mm: 22.0,
                },
                text(
                    "name",
                    TextSource::ProductName,
                    26.0,
                    2.0,
                    30.0,
                    12.0,
                    9.0,
                    true,
                    TextAlign::Left,
                ),
                text(
                    "price",
                    TextSource::Price,
                    26.0,
                    16.0,
                    30.0,
                    8.0,
                    12.0,
                    true,
                    TextAlign::Left,
                ),
                LabelElement::Line {
                    id: "rule".into(),
                    x_mm: 2.0,
                    y_mm: 26.0,
                    width_mm: 54.0,
                    height_mm: 0.3,
                },
                text(
                    "sku",
                    TextSource::Sku,
                    2.0,
                    28.0,
                    54.0,
                    5.0,
                    7.0,
                    true,
                    TextAlign::Left,
                ),
                text(
                    "code",
                    TextSource::Code,
                    2.0,
                    34.0,
                    54.0,
                    4.0,
                    7.0,
                    false,
                    TextAlign::Left,
                ),
            ],
        ),
        starter(
            "qr_sticker_40x30",
            "QR sticker 40 x 30 mm",
            40.0,
            30.0,
            vec![
                LabelElement::Qr {
                    id: "qr".into(),
                    x_mm: 1.0,
                    y_mm: 1.0,
                    width_mm: 20.0,
                    height_mm: 20.0,
                },
                text(
                    "name",
                    TextSource::ProductName,
                    22.0,
                    1.0,
                    17.0,
                    10.0,
                    6.5,
                    true,
                    TextAlign::Left,
                ),
                text(
                    "price",
                    TextSource::Price,
                    22.0,
                    12.0,
                    17.0,
                    6.0,
                    8.0,
                    true,
                    TextAlign::Left,
                ),
                text(
                    "sku",
                    TextSource::Sku,
                    1.0,
                    22.0,
                    38.0,
                    4.0,
                    6.5,
                    true,
                    TextAlign::Left,
                ),
                text(
                    "code",
                    TextSource::Code,
                    1.0,
                    26.0,
                    38.0,
                    3.5,
                    5.5,
                    false,
                    TextAlign::Left,
                ),
            ],
        ),
    ]
}

/// The starter with this key, if any.
pub(crate) fn find(key: &str) -> Option<LabelTemplate> {
    all().into_iter().find(|template| template.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::labels::LabelTemplateInput,
        modules::labels::service::validate::{check_code_fits, validate_template},
    };

    #[test]
    fn every_starter_is_a_valid_design_whose_codes_fit() {
        let starters = all();
        assert_eq!(starters.len(), 3);
        for template in &starters {
            let input = LabelTemplateInput {
                name: template.name.clone(),
                width_mm: template.width_mm,
                height_mm: template.height_mm,
                elements: template.elements.clone(),
            };
            validate_template(&input).unwrap_or_else(|e| panic!("{}: {e}", template.name));
            // The longest code the app issues: a 14-digit manual barcode.
            check_code_fits(&template.elements, "20000000000150", "x")
                .unwrap_or_else(|e| panic!("{}: {e}", template.name));
            assert!(template.built_in && template.key.starts_with(STARTER_KEY_PREFIX));
        }
    }

    #[test]
    fn starter_keys_and_names_are_unique() {
        let starters = all();
        for (i, a) in starters.iter().enumerate() {
            for b in &starters[i + 1..] {
                assert_ne!(a.key, b.key);
                assert_ne!(a.name.to_lowercase(), b.name.to_lowercase());
            }
        }
        assert!(find("starter_shelf_58x40").is_some());
        assert!(find("starter_missing").is_none());
    }
}
