use mongodb::bson;
use mongodb::bson::{Bson, DateTime, Document};
use serde::de::DeserializeOwned;
use serde_path_to_error::Segment;

/// One step from the document root to a nested value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

/// Why a stored document does not load as its typed struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// A required field is absent. `parent` locates the object that lacks it.
    MissingField { parent: Vec<Step>, field: String },
    /// Anything else (wrong type, corrupt value). Never repaired automatically.
    Other(String),
}

/// The fields `repair_document` wrote, as `$set` paths such as `items.0.returned_quantity`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Repair {
    pub set: Vec<(String, Bson)>,
}

/// Loads `document` as `T`, reporting the first missing field with its location.
pub(super) fn probe<T: DeserializeOwned>(document: &Document) -> Result<(), Fault> {
    let deserializer = bson::Deserializer::new(Bson::Document(document.clone()));
    match serde_path_to_error::deserialize::<_, T>(deserializer) {
        Ok(_) => Ok(()),
        Err(err) => Err(fault_of(&err)),
    }
}

fn fault_of(err: &serde_path_to_error::Error<bson::error::Error>) -> Fault {
    let message = err.inner().to_string();
    let Some(field) = missing_field_name(&message) else {
        return Fault::Other(format!("{} at {}", message, err.path()));
    };
    let mut parent = Vec::new();
    for segment in err.path().iter() {
        match segment {
            Segment::Map { key } => parent.push(Step::Key(key.clone())),
            Segment::Seq { index } => parent.push(Step::Index(*index)),
            Segment::Enum { .. } | Segment::Unknown => {
                return Fault::Other(format!("{} at {}", message, err.path()));
            }
        }
    }
    Fault::MissingField { parent, field }
}

/// The name in serde's "missing field `name`" message.
fn missing_field_name(message: &str) -> Option<String> {
    let rest = message.split_once("missing field `")?.1;
    Some(rest.split_once('`')?.0.to_string())
}

/// The fields, per collection, that had `#[serde(default)]` before it was removed (nested
/// structs included). Only these may be backfilled; any other missing field is a real defect.
const BACKFILLABLE: &[(&str, &[&str])] = &[
    ("account_openings", &["created_at", "updated_at", "version"]),
    (
        "advance_recoveries",
        &["created_at", "updated_at", "version"],
    ),
    ("attachments", &["created_at", "updated_at", "version"]),
    ("balance_openings", &["created_at", "updated_at", "version"]),
    ("cash_categories", &["created_at", "updated_at", "version"]),
    (
        "cash_movements",
        &["created_at", "recorded_late", "updated_at", "version"],
    ),
    ("cash_settings", &["created_at", "updated_at", "version"]),
    ("categories", &["created_at", "updated_at", "version"]),
    (
        "credit_notes",
        &[
            "balance_reduction_cents",
            "batch_allocations",
            "created_at",
            "exchange_items",
            "is_manager_override",
            "no_receipt",
            "refund_breakdown",
            "refund_cash_cents",
            "refund_payment_keys",
            "returned_quantity",
            "serial_numbers",
            "updated_at",
            "version",
        ],
    ),
    (
        "customers",
        &[
            "created_at",
            "outstanding_balance_cents",
            "tags",
            "total_purchases_cents",
            "updated_at",
            "version",
        ],
    ),
    (
        "drawer_state",
        &["created_at", "key", "updated_at", "version"],
    ),
    (
        "employees",
        &["created_at", "status", "updated_at", "version"],
    ),
    ("expense_payments", &["created_at", "updated_at", "version"]),
    ("expenses", &["created_at", "updated_at", "version"]),
    (
        "generated_documents",
        &["created_at", "template_key", "updated_at"],
    ),
    ("import_batches", &["created_at", "errors", "updated_at"]),
    ("incomes", &["created_at", "updated_at", "version"]),
    (
        "invoices",
        &[
            "batch_allocations",
            "created_at",
            "credit_note_count",
            "discount_type",
            "discount_value",
            "refunded_cents",
            "returned_quantity",
            "serial_numbers",
            "updated_at",
            "version",
        ],
    ),
    ("label_print_jobs", &["created_at", "updated_at"]),
    ("label_templates", &["created_at", "updated_at", "version"]),
    ("login_sessions", &["created_at", "updated_at"]),
    ("payees", &["created_at", "updated_at", "version"]),
    ("payments", &["created_at", "updated_at", "version"]),
    ("print_jobs", &["created_at", "updated_at", "version"]),
    ("product_serials", &["created_at", "updated_at", "version"]),
    (
        "products",
        &[
            "category_key",
            "created_at",
            "is_serialized",
            "min_stock_threshold",
            "name",
            "sku",
            "stock_quantity",
            "subcategory_key",
            "updated_at",
            "version",
        ],
    ),
    (
        "purchase_receipts",
        &["created_at", "updated_at", "version"],
    ),
    (
        "purchases",
        &["charges_share_cents", "created_at", "updated_at", "version"],
    ),
    (
        "quotations",
        &[
            "created_at",
            "discount_cents",
            "has_out_of_stock_items",
            "updated_at",
            "version",
        ],
    ),
    ("receipt_charges", &["created_at", "updated_at", "version"]),
    (
        "receipt_settlements",
        &["created_at", "updated_at", "version"],
    ),
    (
        "recurring_expenses",
        &["created_at", "updated_at", "version"],
    ),
    (
        "register_sessions",
        &[
            "created_at",
            "has_late_movements",
            "opening_discrepancy_cents",
            "total_card_sales_cents",
            "total_cash_drops_cents",
            "total_cash_refunds_cents",
            "total_cash_sales_cents",
            "total_credit_sales_cents",
            "total_online_sales_cents",
            "total_owner_withdrawals_cents",
            "total_pay_in_cents",
            "total_pay_out_cents",
            "updated_at",
            "version",
        ],
    ),
    ("repairs", &["created_at", "updated_at", "version"]),
    ("sequence_counters", &["next_val"]),
    (
        "shop_profiles",
        &[
            "account_name",
            "account_number",
            "address_lines",
            "allow_shared_user_sessions",
            "bank_branch",
            "bank_name",
            "business_reg_no",
            "created_at",
            "default_footer_text",
            "default_warranty_text",
            "email",
            "legal_name",
            "logo_base64",
            "primary_phone",
            "receipt_footer_text",
            "secondary_phone",
            "trading_name",
            "updated_at",
            "version",
            "website",
        ],
    ),
    ("stock_batches", &["created_at", "updated_at", "version"]),
    ("stock_movements", &["updated_at", "version"]),
    ("subcategories", &["created_at", "updated_at", "version"]),
    ("supplier_credits", &["created_at", "updated_at", "version"]),
    (
        "supplier_products",
        &["created_at", "updated_at", "version"],
    ),
    (
        "suppliers",
        &["contact_persons", "created_at", "updated_at", "version"],
    ),
    (
        "tenants",
        &[
            "created_at",
            "sample_data_loaded",
            "setup_completed",
            "updated_at",
        ],
    ),
    ("users", &["created_at", "is_active", "updated_at"]),
];

/// Whether `field` of `collection` may be backfilled.
fn is_backfillable(collection: &str, field: &str) -> bool {
    BACKFILLABLE
        .iter()
        .any(|(name, fields)| *name == collection && fields.contains(&field))
}

/// The value an old document gets for a field that did not exist when it was written.
/// This is the one place the former `#[serde(default)]` values live, written into the
/// data once instead of invented on every read. `None` means the field has no safe
/// value and the document must be fixed by hand.
fn backfill_value(field: &str, object: &Document) -> Option<Bson> {
    let value = match field {
        "version" => Bson::Int64(1),
        "key" => object.get("user_key").cloned()?,
        "created_at" => object
            .get("updated_at")
            .cloned()
            .unwrap_or_else(|| Bson::DateTime(DateTime::now())),
        "updated_at" => object
            .get("created_at")
            .cloned()
            .unwrap_or_else(|| Bson::DateTime(DateTime::now())),
        "is_active" => Bson::Boolean(true),
        "discount_type" => Bson::String("fixed".to_string()),
        "status" => Bson::String("active".to_string()),
        "discount_value" => Bson::Double(0.0),
        "address_lines"
        | "batch_allocations"
        | "contact_persons"
        | "errors"
        | "exchange_items"
        | "refund_breakdown"
        | "refund_payment_keys"
        | "serial_numbers"
        | "tags" => Bson::Array(Vec::new()),
        "options" | "selections" => Bson::Document(Document::new()),
        "allow_shared_user_sessions"
        | "has_late_movements"
        | "has_out_of_stock_items"
        | "is_manager_override"
        | "is_overdue"
        | "is_serialized"
        | "no_receipt"
        | "recorded_late" => Bson::Boolean(false),
        "account_name"
        | "account_number"
        | "bank_branch"
        | "bank_name"
        | "business_reg_no"
        | "category_key"
        | "default_footer_text"
        | "default_warranty_text"
        | "email"
        | "legal_name"
        | "logo_base64"
        | "name"
        | "primary_phone"
        | "receipt_footer_text"
        | "secondary_phone"
        | "sku"
        | "subcategory_key"
        | "template_key"
        | "trading_name"
        | "website" => Bson::String(String::new()),
        "balance_reduction_cents"
        | "charges_share_cents"
        | "credit_note_count"
        | "discount_cents"
        | "min_stock_threshold"
        | "next_val"
        | "opening_discrepancy_cents"
        | "outstanding_balance_cents"
        | "refund_cash_cents"
        | "refunded_cents"
        | "returned_quantity"
        | "stock_quantity"
        | "total_card_sales_cents"
        | "total_cash_drops_cents"
        | "total_cash_refunds_cents"
        | "total_cash_sales_cents"
        | "total_credit_sales_cents"
        | "total_online_sales_cents"
        | "total_owner_withdrawals_cents"
        | "total_pay_in_cents"
        | "total_pay_out_cents"
        | "total_purchases_cents" => Bson::Int64(0),
        _ => return None,
    };
    Some(value)
}

/// The object at `path` below `root`, if every step exists.
fn object_at<'a>(root: &'a mut Document, path: &[Step]) -> Option<&'a mut Document> {
    let mut object = root;
    let mut steps = path.iter();
    while let Some(step) = steps.next() {
        let Step::Key(key) = step else { return None };
        let child = object.get_mut(key)?;
        match (child, steps.clone().next()) {
            (Bson::Document(inner), _) => object = inner,
            (Bson::Array(items), Some(Step::Index(index))) => {
                steps.next();
                let Bson::Document(inner) = items.get_mut(*index)? else {
                    return None;
                };
                object = inner;
            }
            _ => return None,
        }
    }
    Some(object)
}

/// The `$set` path of `field` inside the object at `parent`.
fn set_path(parent: &[Step], field: &str) -> String {
    let mut path = String::new();
    for step in parent {
        match step {
            Step::Key(key) => {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
            }
            Step::Index(index) => {
                path.push('.');
                path.push_str(&index.to_string());
            }
        }
    }
    if !path.is_empty() {
        path.push('.');
    }
    path.push_str(field);
    path
}

/// Fills every missing field of `document` until it loads with `check`.
///
/// Only fields that used to carry `#[serde(default)]` in `collection` are filled.
///
/// # Errors
/// Returns the fault that has no backfill value, or that is not a missing field.
pub fn repair_document(
    collection: &str,
    document: &mut Document,
    check: fn(&Document) -> Result<(), Fault>,
) -> Result<Repair, Fault> {
    const MAX_FIELDS: usize = 256;
    let mut repair = Repair::default();
    for _ in 0..MAX_FIELDS {
        match check(document) {
            Ok(()) => return Ok(repair),
            Err(Fault::MissingField { parent, field }) => {
                let target = object_at(document, &parent)
                    .ok_or_else(|| Fault::Other(format!("no object at the path of `{field}`")))?;
                let value = is_backfillable(collection, &field)
                    .then(|| backfill_value(&field, target))
                    .flatten()
                    .ok_or_else(|| {
                        Fault::Other(format!("`{field}` is missing and has no backfill value"))
                    })?;
                target.insert(field.clone(), value.clone());
                repair.set.push((set_path(&parent, &field), value));
            }
            Err(other) => return Err(other),
        }
    }
    Err(Fault::Other("more than 256 missing fields".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::doc;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Line {
        #[allow(dead_code)]
        returned_quantity: i64,
    }

    #[derive(Debug, Deserialize)]
    struct Parent {
        #[allow(dead_code)]
        version: i64,
        #[allow(dead_code)]
        items: Vec<Line>,
    }

    #[test]
    fn a_missing_top_level_field_is_reported_and_backfilled() {
        let mut document = doc! { "items": [] };
        assert_eq!(
            probe::<Parent>(&document),
            Err(Fault::MissingField {
                parent: Vec::new(),
                field: "version".to_string()
            })
        );
        let repair =
            repair_document("customers", &mut document, probe::<Parent>).expect("repairable");
        assert_eq!(repair.set, vec![("version".to_string(), Bson::Int64(1))]);
        assert_eq!(probe::<Parent>(&document), Ok(()));
    }

    #[test]
    fn a_missing_field_inside_an_array_item_is_located_and_backfilled() {
        let mut document = doc! { "version": 3_i64, "items": [{ "returned_quantity": 1_i64 }, {}] };
        let repair =
            repair_document("invoices", &mut document, probe::<Parent>).expect("repairable");
        assert_eq!(
            repair.set,
            vec![("items.1.returned_quantity".to_string(), Bson::Int64(0))]
        );
    }

    #[test]
    fn a_complete_document_needs_no_repair() {
        let mut document = doc! { "version": 1_i64, "items": [] };
        let repair = repair_document("customers", &mut document, probe::<Parent>).expect("loads");
        assert!(repair.set.is_empty());
    }

    #[test]
    fn a_field_without_a_backfill_value_is_refused() {
        #[derive(Debug, Deserialize)]
        struct Unknown {
            #[allow(dead_code)]
            surprise: i64,
        }
        let mut document = doc! {};
        let fault =
            repair_document("customers", &mut document, probe::<Unknown>).expect_err("no value");
        assert!(matches!(fault, Fault::Other(_)));
    }
}
