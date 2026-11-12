use mongodb::bson::{Bson, Document, doc};
use mongodb::options::UpdateModifications;
use std::future::Future;
use std::sync::Arc;

/// Origin of a write made outside any device or browser session (background
/// work, internal provisioning). No device ever has this id, so every device
/// receives the change.
pub const CLOUD_ORIGIN: &str = "cloud";

tokio::task_local! {
    pub(super) static CURRENT_ORIGIN: Arc<str>;
}

/// The writer of the running request, if one was put in scope.
pub fn current_origin() -> Option<Arc<str>> {
    CURRENT_ORIGIN.try_with(Clone::clone).ok()
}

#[must_use]
/// Whether `name` is a collection that takes part in sync.
pub fn is_synced_collection(name: &str) -> bool {
    SYNCED_COLLECTIONS.contains(&name)
}

/// Field every synced document carries.
pub const ORIGIN_FIELD: &str = "updated_by_device";

/// The origin for a request: a registered device's id, else the browser's
/// `X-Device-Id` (prefixed), else `cloud`.
pub fn origin_for(device_id: Option<&str>, header_device: Option<&str>) -> String {
    if let Some(d) = device_id.map(str::trim).filter(|d| !d.is_empty()) {
        return d.to_string();
    }
    match header_device.map(str::trim).filter(|d| !d.is_empty()) {
        Some(h) => format!("{WEB_PREFIX}{h}"),
        None => CLOUD_ORIGIN.to_string(),
    }
}

/// Sets the origin on a document about to be inserted or used as a
/// replacement.
#[must_use]
pub fn stamp_document(origin: &str, mut document: Document) -> Document {
    document.insert(ORIGIN_FIELD, origin);
    document
}

/// Sets the origin on an update. Operator documents get it folded into
/// `$set` (an update with no `$set` gains one); pipeline updates get a final
/// `$set` stage. A replacement-style document (no operators) is stamped like
/// an inserted one.
#[must_use]
pub fn stamp_update(origin: &str, update: UpdateModifications) -> UpdateModifications {
    match update {
        UpdateModifications::Document(mut d) => {
            let is_operator_doc = d.keys().next().is_some_and(|k| k.starts_with('$'));
            if !is_operator_doc {
                return UpdateModifications::Document(stamp_document(origin, d));
            }
            // A `$setOnInsert`/`$unset` of the same field would conflict with
            // the `$set` below; the request's origin wins either way.
            for op in ["$setOnInsert", "$unset"] {
                if let Some(Bson::Document(inner)) = d.get_mut(op) {
                    inner.remove(ORIGIN_FIELD);
                }
                if matches!(d.get(op), Some(Bson::Document(inner)) if inner.is_empty()) {
                    d.remove(op);
                }
            }
            match d.get_mut("$set") {
                Some(Bson::Document(set)) => {
                    set.insert(ORIGIN_FIELD, origin);
                }
                _ => {
                    d.insert("$set", doc! { ORIGIN_FIELD: origin });
                }
            }
            UpdateModifications::Document(d)
        }
        UpdateModifications::Pipeline(mut stages) => {
            stages.push(doc! { "$set": { ORIGIN_FIELD: origin } });
            UpdateModifications::Pipeline(stages)
        }
        other => other,
    }
}

/// Mongo collections whose writes are published to the sync change log: the
/// tables of `modules::sync::resources::SYNC_RESOURCES` plus `subcategories`
/// (republished as their parent category). Kept here so `clients` need not
/// depend on `modules`; `tests` in `modules::sync::resources` guard the match.
pub const SYNCED_COLLECTIONS: &[&str] = &[
    "categories",
    "subcategories",
    "suppliers",
    "products",
    "stock_batches",
    "supplier_products",
    "employees",
    "customers",
    "purchase_receipts",
    "receipt_settlements",
    "supplier_credits",
    "receipt_charges",
    "purchases",
    "stock_movements",
    "product_serials",
    "repairs",
    "print_jobs",
    "invoices",
    "payments",
    "credit_notes",
    "quotations",
    "shop_profiles",
    "register_sessions",
    "cash_movements",
    "drawer_state",
    "cash_categories",
    "payees",
    "expenses",
    "expense_payments",
    "incomes",
    "account_openings",
    "balance_openings",
    "attachments",
    "cash_settings",
    "advance_recoveries",
    "recurring_expenses",
    "users",
    "warranties",
    "warranty_events",
    "warranty_claims",
    "repair_service_templates",
];

/// Prefix for a browser session's `X-Device-Id`. Keeps a browser from ever
/// sharing an id with a registered device (which would hide that device's
/// own data from it).
pub const WEB_PREFIX: &str = "web:";

/// Runs `fut` with `origin` as the writer of every synced document it touches.
pub async fn with_origin<F: Future>(origin: impl Into<Arc<str>>, fut: F) -> F::Output {
    CURRENT_ORIGIN.scope(origin.into(), fut).await
}
