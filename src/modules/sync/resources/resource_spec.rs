use super::sync_resources::SYNC_RESOURCES;

/// The column that names a row to a person (invoice number, product name…).
/// `None` for rows that have no readable name of their own (payments, stock
/// movements…). Fixed strings only: the result is spliced into SQL.
#[must_use]
pub fn label_column(table: &str) -> Option<&'static str> {
    match table {
        "invoices" => Some("invoice_number"),
        "credit_notes" => Some("credit_note_number"),
        "quotations" => Some("quotation_number"),
        "repairs" | "print_jobs" => Some("ticket_number"),
        "purchases" => Some("receipt_key"),
        "purchase_receipts" => Some("receipt_number"),
        "products" | "customers" | "suppliers" | "employees" | "categories" | "users" => {
            Some("name")
        }
        "product_serials" => Some("serial_number"),
        "register_sessions" => Some("session_number"),
        "cash_movements" => Some("movement_number"),
        "cash_categories" | "payees" => Some("name"),
        "expenses" => Some("expense_number"),
        "receipt_settlements" => Some("payment_key"),
        "supplier_credits" => Some("key"),
        "receipt_charges" => Some("key"),
        "warranties" => Some("warranty_number"),
        "warranty_events" => Some("key"),
        "warranty_claims" => Some("claim_number"),
        _ => None,
    }
}

// Single source of truth for which resources sync, how each merges, which
// columns are derived (never taken from a payload) and the order changes are
// applied in. Shared by the device (SQLite) and cloud (Mongo) sides so both
// agree by construction. The older `SYNCABLE` list in `service.rs` keeps
// serving the legacy `/sync/changes` route and is intentionally left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How concurrent edits of a resource are merged.
pub enum MergeClass {
    /// Rows are only ever inserted: union by `key`.
    AppendOnly,
    /// Whole-row last-writer-wins by `(updated_at, device_id)`.
    Lww,
    /// LWW on ordinary columns; `derived` columns are recomputed from ledgers.
    LwwWithDerived,
    /// Body immutable; lifecycle columns merge monotonically (invoices, credit notes, quotations).
    Lifecycle,
}

/// Resources in apply order (parents first).
pub fn ordered() -> impl Iterator<Item = &'static ResourceSpec> {
    SYNC_RESOURCES.iter()
}

/// Rollout phase of a resource. Every resource, `users` included, now syncs in
/// `P3a`; `P3b` is kept for a future resource that must be held back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    P3a,
    P3b,
}

#[derive(Debug)]
/// How one synced resource is named, stored and merged.
pub struct ResourceSpec {
    /// Wire name (camelCase), e.g. `stockMovements`.
    pub name: &'static str,
    /// SQLite table == Mongo collection.
    pub table: &'static str,
    pub class: MergeClass,
    /// Has a `deleted_at` column (deletes are tombstones).
    pub soft_delete: bool,
    /// snake_case columns never taken from a payload.
    pub derived: &'static [&'static str],
    /// Columns never put on the wire (none in P3a).
    pub local_only: &'static [&'static str],
    /// Apply order, ascending: parents before children.
    pub order: u8,
    pub phase: Phase,
}

/// Payload fields that travel between a device and the cloud but must never be
/// written anywhere else (conflict records, logs): the `users` password hash.
/// The hash is needed on the wire so a cashier created on one device can log in
/// on every other device and on the cloud.
#[must_use]
pub fn secret_fields(resource: &str) -> &'static [&'static str] {
    match resource {
        "users" => &["passwordHash", "pinHash"],
        _ => &[],
    }
}

/// `snake_case` column name -> `camelCase` DTO field name.
#[must_use]
pub fn snake_to_camel(column: &str) -> String {
    let mut out = String::with_capacity(column.len());
    let mut upper = false;
    for c in column.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Looks a resource up by wire name or table name.
#[must_use]
pub fn spec(name: &str) -> Option<&'static ResourceSpec> {
    SYNC_RESOURCES
        .iter()
        .find(|s| s.name == name || s.table == name)
}

/// A copy of `payload` without the resource's secret fields, for anything that
/// persists a payload outside the resource's own table (conflict details).
#[must_use]
pub fn without_secrets(resource: &str, payload: &serde_json::Value) -> serde_json::Value {
    let mut copy = payload.clone();
    if let Some(object) = copy.as_object_mut() {
        for field in secret_fields(resource) {
            object.remove(*field);
        }
    }
    copy
}
