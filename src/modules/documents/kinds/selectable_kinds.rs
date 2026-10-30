use super::document_kind::DocumentKind;

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
    DocumentKind {
        slug: "z-report",
        label: "Z-Report",
    },
];
