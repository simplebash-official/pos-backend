use super::document_selection::DocumentSelection;

pub(super) fn to_selections(pairs: Vec<(String, String)>) -> Vec<DocumentSelection> {
    pairs
        .into_iter()
        .map(|(kind, family)| DocumentSelection { kind, family })
        .collect()
}
