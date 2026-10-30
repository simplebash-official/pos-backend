use super::document_kind::DocumentKind;
use super::selectable_kinds::SELECTABLE_KINDS;

pub(crate) fn find(slug: &str) -> Option<&'static DocumentKind> {
    SELECTABLE_KINDS.iter().find(|kind| kind.slug == slug)
}
