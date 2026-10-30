use crate::domain::labels::{LabelElement, TextSource};

pub(super) fn has_price_element(elements: &[LabelElement]) -> bool {
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
