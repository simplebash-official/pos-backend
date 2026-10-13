// The kinds of printed document a shop can choose a template for. A *kind*
// is what a template is for (`x-document-kind` in its schema on the
// document-server); every template of a kind accepts that kind's standard
// payload, so any of them can print the same invoice, receipt or credit
// note. `analytics-report` is a kind too, but it is not a shop choice and so
// is not listed here.

/// One selectable document kind.
pub(crate) struct DocumentKind {
    /// The slug used in routes (`documentType`), storage and the schema marker.
    pub slug: &'static str,
    /// What the Settings screen calls it.
    pub label: &'static str,
}

pub(crate) const SELECTABLE_KINDS: &[DocumentKind] = &[
    DocumentKind {
        slug: "a4-invoice",
        label: "A4 Invoice",
    },
    DocumentKind {
        slug: "thermal-receipt",
        label: "Receipt",
    },
    DocumentKind {
        slug: "credit-note",
        label: "Credit Note",
    },
    DocumentKind {
        slug: "quotation",
        label: "Quotation",
    },
];

pub(crate) fn find(slug: &str) -> Option<&'static DocumentKind> {
    SELECTABLE_KINDS.iter().find(|kind| kind.slug == slug)
}
