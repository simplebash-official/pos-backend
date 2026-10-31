use super::resource_spec::MergeClass;
use super::resource_spec::Phase;
use super::resource_spec::label_column;
use super::resource_spec::snake_to_camel;
use super::resource_spec::spec;
use super::resource_spec::without_secrets;
use super::sync_resources::SYNC_RESOURCES;

#[test]
fn origin_stamped_collections_are_exactly_the_synced_tables() {
    let mut expected: Vec<&str> = SYNC_RESOURCES.iter().map(|s| s.table).collect();
    expected.push("subcategories");
    expected.sort_unstable();
    let mut actual = crate::core::sync_origin::SYNCED_COLLECTIONS.to_vec();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}

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
fn quotations_sync_as_an_undeletable_lifecycle_resource_named_by_number() {
    let s = spec("quotations").expect("quotations must sync");
    assert_eq!(s.table, "quotations");
    assert_eq!(s.class, MergeClass::Lifecycle);
    assert!(!s.soft_delete && s.derived.is_empty());
    assert_eq!(label_column("quotations"), Some("quotation_number"));
    // Applied after the invoices/credit notes it sits beside, before users.
    assert!(spec("creditNotes").unwrap().order < s.order);
    assert!(s.order < spec("users").unwrap().order);
}

#[test]
fn every_resource_including_users_syncs_in_p3a() {
    assert!(SYNC_RESOURCES.iter().all(|s| s.phase == Phase::P3a));
    assert!(spec("users").is_some());
}

#[test]
fn the_password_hash_is_stripped_from_stored_payload_copies() {
    let payload =
        serde_json::json!({ "key": "usr_1", "passwordHash": "$argon2id$secret", "name": "A" });
    let clean = without_secrets("users", &payload);
    assert!(clean.get("passwordHash").is_none());
    assert_eq!(clean["name"], "A");
    assert_eq!(without_secrets("products", &payload), payload);
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
