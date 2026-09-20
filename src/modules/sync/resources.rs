// Single source of truth for which resources sync, how each merges, which
// columns are derived (never taken from a payload) and the order changes are
// applied in. Shared by the device (SQLite) and cloud (Mongo) sides so both
// agree by construction. The older `SYNCABLE` list in `service.rs` keeps
// serving the legacy `/sync/changes` route and is intentionally left alone.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeClass {
    /// Rows are only ever inserted: union by `key`.
    AppendOnly,
    /// Whole-row last-writer-wins by `(updated_at, device_id)`.
    Lww,
    /// LWW on ordinary columns; `derived` columns are recomputed from ledgers.
    LwwWithDerived,
    /// Body immutable; lifecycle columns merge monotonically (invoices, credit notes).
    Lifecycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    P3a,
    P3b,
}

#[derive(Debug)]
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

pub const SYNC_RESOURCES: &[ResourceSpec] = &[
    ResourceSpec {
        name: "categories",
        table: "categories",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 10,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "suppliers",
        table: "suppliers",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 20,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "products",
        table: "products",
        class: MergeClass::LwwWithDerived,
        soft_delete: true,
        derived: &["stock_quantity"],
        local_only: &[],
        order: 30,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "supplierProducts",
        table: "supplier_products",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 40,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "employees",
        table: "employees",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 50,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "customers",
        table: "customers",
        class: MergeClass::LwwWithDerived,
        soft_delete: true,
        derived: &["outstanding_balance_cents", "total_purchases_cents"],
        local_only: &[],
        order: 60,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "purchases",
        table: "purchases",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 70,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "stockMovements",
        table: "stock_movements",
        class: MergeClass::AppendOnly,
        soft_delete: false,
        derived: &[],
        local_only: &[],
        order: 80,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "productSerials",
        table: "product_serials",
        class: MergeClass::Lww,
        soft_delete: false,
        derived: &[],
        local_only: &[],
        order: 85,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "repairs",
        table: "repairs",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 90,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "printJobs",
        table: "print_jobs",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 100,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "invoices",
        table: "invoices",
        class: MergeClass::Lifecycle,
        soft_delete: false,
        derived: &[
            "refunded_cents",
            "credit_note_count",
            "items[].returned_quantity",
            "status",
        ],
        local_only: &[],
        order: 110,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "payments",
        table: "payments",
        class: MergeClass::AppendOnly,
        soft_delete: false,
        derived: &[],
        local_only: &[],
        order: 120,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "creditNotes",
        table: "credit_notes",
        class: MergeClass::Lifecycle,
        soft_delete: false,
        derived: &[],
        local_only: &[],
        order: 130,
        phase: Phase::P3a,
    },
    ResourceSpec {
        name: "users",
        table: "users",
        class: MergeClass::Lww,
        soft_delete: true,
        derived: &[],
        local_only: &[],
        order: 140,
        phase: Phase::P3b,
    },
];

/// Looks a resource up by wire name or table name.
pub fn spec(name: &str) -> Option<&'static ResourceSpec> {
    SYNC_RESOURCES
        .iter()
        .find(|s| s.name == name || s.table == name)
}

/// Resources in apply order (parents first).
pub fn ordered() -> impl Iterator<Item = &'static ResourceSpec> {
    SYNC_RESOURCES.iter()
}

/// `snake_case` column name -> `camelCase` DTO field name.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_by_order_and_orders_are_unique() {
        let orders: Vec<u8> = SYNC_RESOURCES.iter().map(|s| s.order).collect();
        let mut sorted = orders.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(orders, sorted);
    }

    #[test]
    fn spec_accepts_wire_and_table_names() {
        assert_eq!(spec("stockMovements").unwrap().table, "stock_movements");
        assert_eq!(spec("stock_movements").unwrap().name, "stockMovements");
        assert!(spec("login_sessions").is_none());
        assert!(spec("subcategories").is_none());
    }

    #[test]
    fn append_only_and_lifecycle_resources_have_no_tombstones() {
        for s in SYNC_RESOURCES {
            if matches!(s.class, MergeClass::AppendOnly | MergeClass::Lifecycle) {
                assert!(!s.soft_delete, "{} must not soft delete", s.name);
            }
        }
    }

    #[test]
    fn derived_columns_only_on_derived_capable_classes() {
        for s in SYNC_RESOURCES {
            if !s.derived.is_empty() {
                assert!(
                    matches!(s.class, MergeClass::LwwWithDerived | MergeClass::Lifecycle),
                    "{}",
                    s.name
                );
            }
        }
    }

    #[test]
    fn users_are_the_only_p3b_resource() {
        let p3b: Vec<_> = SYNC_RESOURCES
            .iter()
            .filter(|s| s.phase == Phase::P3b)
            .map(|s| s.name)
            .collect();
        assert_eq!(p3b, ["users"]);
    }

    #[test]
    fn snake_to_camel_converts() {
        assert_eq!(snake_to_camel("stock_quantity"), "stockQuantity");
        assert_eq!(
            snake_to_camel("outstanding_balance_cents"),
            "outstandingBalanceCents"
        );
        assert_eq!(snake_to_camel("status"), "status");
    }
}
