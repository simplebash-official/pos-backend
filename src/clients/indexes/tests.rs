use super::clients_indexes::IndexSpec;
use super::index_specs::index_specs;
use crate::core::config::TenantMode;
use crate::core::tenancy::TENANT_FIELD;
use mongodb::bson::doc;

fn names(specs: &[IndexSpec]) -> Vec<String> {
    specs
        .iter()
        .map(|s| format!("{}.{}", s.collection, s.name))
        .collect()
}

#[test]
fn single_tenant_specs_are_the_historical_set() {
    let specs = index_specs(TenantMode::Single);
    // 17 sync + 4 created_at + 2 invoice analytics + 3 reminders + products_key
    // + idempotency unique/ttl + serial unique + serial status
    // + stock_batches product_key + shop_profiles + document_template_settings
    // + label_templates + label_print_jobs key/created_at + invoices session_key
    // + cash_categories key/type-name + payees key/type-name + cash_movements category/payee
    // + expenses key/date + expense_payments expense_key + incomes key/date + account_openings key + balance_openings key + advance_recoveries key/employee_key + attachments key/owner + cash_settings key + purchase_receipts key/supplier-reference + purchases receipt_key
    // + receipt_settlements key/receipt_key/payment_key + supplier_credits key/supplier_key/receipt_key
    // + receipt_charges key/receipt_key/supplier_key + drawer_state user_key.
    assert_eq!(specs.len(), 70);
    assert!(specs.iter().all(|s| s.keys.get(TENANT_FIELD).is_none()));
    assert!(names(&specs).contains(&"products.sync_updated_at_key".to_string()));
    assert!(names(&specs).contains(&"idempotency_keys.idempotency_ttl".to_string()));

    let unique: Vec<_> = specs
        .iter()
        .filter(|s| s.unique)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        unique,
        [
            "idempotency_key_user_unique",
            "product_serials_serial_number_unique",
            "cash_categories_key_unique",
            "payees_key_unique",
            "purchase_receipts_key_unique",
            "receipt_settlements_key_unique",
            "supplier_credits_key_unique",
            "receipt_charges_key_unique",
            "expenses_key_unique",
            "recurring_expenses_key_unique",
            "incomes_key_unique",
            "account_openings_key_unique",
            "balance_openings_key_unique",
            "advance_recoveries_key_unique",
            "attachments_key_unique",
            "cash_settings_key_unique",
            "shop_profiles_key_unique",
            "document_template_settings_key_unique",
            "label_templates_key_unique",
            "label_print_jobs_key_unique",
        ]
    );
    let sync = specs
        .iter()
        .find(|s| s.name == "sync_updated_at_key")
        .unwrap();
    assert_eq!(sync.keys, doc! { "updated_at": 1, "key": 1 });
}

#[test]
fn multi_tenant_indexes_lead_with_tenant_id_except_ttl_and_platform() {
    let specs = index_specs(TenantMode::Multi);
    for spec in &specs {
        match spec.collection {
            "tenants" => assert_eq!(spec.keys, doc! { "shop_code": 1 }),
            "idempotency_keys" if spec.ttl.is_some() => {
                assert_eq!(spec.keys, doc! { "created_at": 1 })
            }
            "sync_batches" if spec.ttl.is_some() => assert_eq!(spec.keys, doc! { "at": 1 }),
            "sync_tombstones" if spec.ttl.is_some() => {
                assert_eq!(spec.keys, doc! { "deleted_at": 1 })
            }
            _ => {
                assert_eq!(
                    spec.keys.keys().next().map(String::as_str),
                    Some(TENANT_FIELD),
                    "{} must lead with tenant_id",
                    spec.name
                );
                assert!(spec.name.starts_with("t_"), "{}", spec.name);
            }
        }
    }
}

#[test]
fn multi_tenant_unique_indexes_are_per_tenant() {
    let specs = index_specs(TenantMode::Multi);
    let serial = specs
        .iter()
        .find(|s| s.name == "t_product_serials_serial_number_unique")
        .unwrap();
    assert!(serial.unique);
    assert_eq!(serial.keys, doc! { "tenant_id": 1, "serial_number": 1 });
    let idem = specs
        .iter()
        .find(|s| s.name == "t_idempotency_key_user_unique")
        .unwrap();
    assert!(idem.unique);
    assert_eq!(idem.keys, doc! { "tenant_id": 1, "key": 1, "user_id": 1 });
    let shop = specs.iter().find(|s| s.collection == "tenants").unwrap();
    assert!(shop.unique);
    assert!(
        index_specs(TenantMode::Single)
            .iter()
            .all(|s| s.collection != "tenants")
    );
}

#[test]
fn index_names_are_unique_per_collection() {
    for tenant_mode in [TenantMode::Single, TenantMode::Multi] {
        let mut seen = std::collections::HashSet::new();
        for s in index_specs(tenant_mode) {
            assert!(
                seen.insert((s.collection, s.name.clone())),
                "duplicate {}",
                s.name
            );
        }
    }
}
