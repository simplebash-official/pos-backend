use super::all::all;
use crate::domain::labels::LabelTemplate;

/// The starter with this key, if any.
pub(crate) fn find(key: &str) -> Option<LabelTemplate> {
    all().into_iter().find(|template| template.key == key)
}
