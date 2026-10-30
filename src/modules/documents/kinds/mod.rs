// The kinds of printed document a shop can choose a template for. A *kind*
// is what a template is for (`x-document-kind` in its schema on the
// document-server); every template of a kind accepts that kind's standard
// payload, so any of them can print the same invoice, receipt or credit
// note. `analytics-report` is a kind too, but it is not a shop choice and so
// is not listed here.

mod document_kind;
mod find;
mod selectable_kinds;

pub(crate) use find::find;
pub(crate) use selectable_kinds::SELECTABLE_KINDS;
