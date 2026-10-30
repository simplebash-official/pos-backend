// Mongo document shapes for the billing feature. Kept separate from
// `domain::billing` (the API-facing types) so BSON concerns like `ObjectId`
// never leak into request/response payloads. `items`/`split_payments`
// embed `domain::billing::InvoiceItem`/`SplitPayment` directly rather than
// duplicating near-identical Mongo-side structs — neither type holds a
// BSON-specific field (no `ObjectId`), so there's nothing a separate
// "document" shape would add. Unit enums (`InvoiceStatus`, `ItemCondition`,
// `ItemDisposition`, `ReturnReason`, `CreditNoteStatus`) are likewise
// embedded directly — their serde `rename_all = "snake_case"` derive
// already produces the plain BSON string Mongo needs.

mod credit_note_document;
mod credit_note_item_document;
mod default_version;
mod invoice_document;
mod payment_document;

pub use credit_note_document::CreditNoteDocument;
pub use credit_note_item_document::CreditNoteItemDocument;
pub use invoice_document::InvoiceDocument;
pub use payment_document::PaymentDocument;
