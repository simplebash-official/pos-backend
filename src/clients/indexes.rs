// Index bootstrap, run once at startup.
//
// Two of these are correctness-critical, not just performance:
//
//   * `(updated_at, key)` is the exact sort `modules::sync::repository`
//     pages by. Without it every `/sync/changes` call is a collection scan,
//     and MongoDB aborts an unindexed in-memory sort above 32 MB — so sync
//     would start failing outright once a shop's history grows.
//   * The unique `(key, user_id)` on `idempotency_keys` is what makes the
//     duplicate-insert branch in `core::middleware::idempotency` reachable
//     at all. Without it `insert_one` always succeeds, both racing requests
//     execute, and the replay guard silently does nothing.
//
// Creating an index that already exists with the same spec is a no-op, so
// this is safe to run on every boot.
//
// The set of indexes is built as plain data (`index_specs`) so it can be unit
// tested. With `multi_tenant` every index leads with `tenant_id` (unique ones
// become unique *per tenant*) and gets a `t_`-prefixed name so it cannot clash
// with a same-named single-tenant index. The idempotency TTL index is the one
// exception: a TTL index must be single-field.

use mongodb::{
    Database, IndexModel,
    bson::{Document, doc},
    options::IndexOptions,
};
use std::time::Duration;

use crate::core::tenancy::TENANT_FIELD;

/// Collections that participate in `GET /sync/changes`.
const SYNCED_COLLECTIONS: &[&str] = &[
    "products",
    "categories",
    "subcategories",
    "suppliers",
    "supplier_products",
    "purchases",
    "stock_movements",
    "customers",
    "employees",
    "repairs",
    "print_jobs",
    "invoices",
    "payments",
    "credit_notes",
    "product_serials",
];

/// How long a completed idempotency record is replayable. Matches the
/// 7-day window the client's outbox assumes when it retries a request whose
/// response was lost.
const IDEMPOTENCY_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How long a push batch's stored acks stay replayable: a device that lost the
/// response retries the same `batchId` well within this window.
const SYNC_BATCH_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How long a hard-delete marker outlives its publication. The consumer reads
/// it within moments; the slack only covers a consumer outage.
const SYNC_TOMBSTONE_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// One index to ensure: enough to build an `IndexModel` and to assert on in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexSpec {
    pub collection: &'static str,
    pub name: String,
    pub keys: Document,
    pub unique: bool,
    pub ttl: Option<Duration>,
}

/// Collects the specs, applying the tenant prefix when `multi_tenant`.
struct Specs {
    multi_tenant: bool,
    out: Vec<IndexSpec>,
}

impl Specs {
    /// A tenant-owned index: prefixed with `tenant_id` in multi-tenant mode.
    fn add(&mut self, collection: &'static str, name: &str, keys: Document, unique: bool) {
        let (name, keys) = if self.multi_tenant {
            let mut scoped = doc! { TENANT_FIELD: 1 };
            scoped.extend(keys);
            (format!("t_{name}"), scoped)
        } else {
            (name.to_string(), keys)
        };
        self.out.push(IndexSpec {
            collection,
            name,
            keys,
            unique,
            ttl: None,
        });
    }

    /// An index that is never tenant-prefixed (TTL indexes must be single-field).
    fn add_untenanted(&mut self, spec: IndexSpec) {
        self.out.push(spec);
    }
}

/// Every index `ensure_indexes` creates. Single-tenant output is exactly the
/// historical set; see the module comment for what changes in multi-tenant mode.
///
/// Unique indexes that exist in MongoDB today are only `idempotency_keys
/// (key, user_id)` and `product_serials.serial_number`; both become
/// tenant-qualified. Uniqueness of `sku`, category/supplier names, user emails
/// and invoice/credit-note/ticket numbers is enforced in the service layer on
/// MongoDB (the SQLite schema has the table-level UNIQUEs), and services see
/// only the tenant's rows, so it is already per tenant.
pub fn index_specs(multi_tenant: bool) -> Vec<IndexSpec> {
    let mut specs = Specs {
        multi_tenant,
        out: Vec::new(),
    };

    for collection in SYNCED_COLLECTIONS {
        specs.add(
            collection,
            "sync_updated_at_key",
            doc! { "updated_at": 1, "key": 1 },
            false,
        );
    }

    // Analytics reporting (`modules::reports::repository::analytics`) matches
    // and buckets on `created_at` across whole collections. Without these the
    // POS has no `created_at` index at all and every analytics range query is
    // a full collection scan.
    for collection in ["invoices", "credit_notes", "purchases", "payments"] {
        specs.add(
            collection,
            "analytics_created_at",
            doc! { "created_at": 1 },
            false,
        );
    }

    // Per-cashier and per-customer analytics slices scan `invoices` filtered
    // by one id and a date range.
    specs.add(
        "invoices",
        "analytics_cashier_created_at",
        doc! { "cashier_id": 1, "created_at": 1 },
        false,
    );
    specs.add(
        "invoices",
        "analytics_customer_created_at",
        doc! { "customer_key": 1, "created_at": 1 },
        false,
    );

    // Credit-reminder / receivables reads and repair & print-job reminders.
    specs.add(
        "invoices",
        "reminders_status_due_date",
        doc! { "status": 1, "due_date": 1 },
        false,
    );
    for collection in ["repairs", "print_jobs"] {
        specs.add(
            collection,
            "reminders_promised_ready_at_status",
            doc! { "promised_ready_at": 1, "status": 1 },
            false,
        );
    }

    // `sales-by-category` joins invoice lines to `products` by `key`; the sync
    // index has `key` second, so a `$lookup` on it alone can't use it.
    specs.add("products", "products_key", doc! { "key": 1 }, false);

    // Makes the duplicate-insert branch in `core::middleware::idempotency`
    // reachable at all.
    specs.add(
        "idempotency_keys",
        "idempotency_key_user_unique",
        doc! { "key": 1, "user_id": 1 },
        true,
    );
    specs.add_untenanted(IndexSpec {
        collection: "idempotency_keys",
        name: "idempotency_ttl".to_string(),
        keys: doc! { "created_at": 1 },
        unique: false,
        ttl: Some(IDEMPOTENCY_RETENTION),
    });

    specs.add(
        "product_serials",
        "product_serials_serial_number_unique",
        doc! { "serial_number": 1 },
        true,
    );
    // Backs `GET /products/{key}/serials?status=`.
    specs.add(
        "product_serials",
        "product_serials_product_key_status",
        doc! { "product_key": 1, "status": 1 },
        false,
    );

    // Platform directory (`modules::tenants`): shop codes are globally unique
    // and the collection is not tenant-owned, so it is never tenant-prefixed.
    if multi_tenant {
        specs.add_untenanted(IndexSpec {
            collection: "tenants",
            name: "tenants_shop_code_unique".to_string(),
            keys: doc! { "shop_code": 1 },
            unique: true,
            ttl: None,
        });
    }

    // Sync v2 cloud collections (modules::sync). They only exist on the
    // multi-tenant cloud, so single-tenant output stays the historical set.
    if multi_tenant {
        // Per-tenant monotonic `seq`: the pull cursor. Unique so a replayed
        // change-stream event can never be assigned two sequence numbers.
        specs.add(
            "sync_changes",
            "sync_changes_seq_unique",
            doc! { "seq": 1 },
            true,
        );
        // De-duplicates a change-stream event replayed after a consumer restart.
        specs.add(
            "sync_changes",
            "sync_changes_source_token_unique",
            doc! { "source_token": 1 },
            true,
        );
        // Latest change of one entity (compaction, snapshot consistency checks).
        specs.add(
            "sync_changes",
            "sync_changes_resource_key_seq",
            doc! { "resource": 1, "key": 1, "seq": 1 },
            false,
        );
        specs.add(
            "sync_meta",
            "sync_meta_kind_unique",
            doc! { "kind": 1 },
            true,
        );
        specs.add(
            "sync_device_state",
            "sync_device_state_device_unique",
            doc! { "device_id": 1 },
            true,
        );
        specs.add(
            "sync_batches",
            "sync_batches_device_batch_unique",
            doc! { "device_id": 1, "batch_id": 1 },
            true,
        );
        // A TTL index must be single-field, so it is not tenant-prefixed.
        specs.add_untenanted(IndexSpec {
            collection: "sync_batches",
            name: "sync_batches_ttl".to_string(),
            keys: doc! { "at": 1 },
            unique: false,
            ttl: Some(SYNC_BATCH_RETENTION),
        });
        // Hard-delete markers only live until the consumer has published them.
        specs.add_untenanted(IndexSpec {
            collection: "sync_tombstones",
            name: "sync_tombstones_ttl".to_string(),
            keys: doc! { "deleted_at": 1 },
            unique: false,
            ttl: Some(SYNC_TOMBSTONE_RETENTION),
        });
        specs.add(
            "sync_conflicts",
            "sync_conflicts_key_unique",
            doc! { "key": 1 },
            true,
        );
        specs.add(
            "sync_conflicts",
            "sync_conflicts_detected_at",
            doc! { "detected_at": 1 },
            false,
        );
    }

    specs.out
}

fn to_model(spec: &IndexSpec) -> IndexModel {
    let mut options = IndexOptions::builder()
        .name(spec.name.clone())
        .unique(spec.unique.then_some(true))
        .build();
    options.expire_after = spec.ttl;
    IndexModel::builder()
        .keys(spec.keys.clone())
        .options(options)
        .build()
}

/// Best-effort: a failure here is logged and the server still starts. An
/// index that could not be created makes sync slower, not wrong, and
/// refusing to boot over it would take the shop offline for a problem that
/// resolves itself on the next deploy.
pub async fn ensure_indexes(db: &Database, multi_tenant: bool) {
    for spec in index_specs(multi_tenant) {
        if let Err(err) = db
            .collection::<Document>(spec.collection)
            .create_index(to_model(&spec))
            .await
        {
            tracing::warn!(
                collection = spec.collection,
                index = %spec.name,
                %err,
                "could not create index"
            );
        }
    }

    tracing::info!("Database indexes ensured");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(specs: &[IndexSpec]) -> Vec<String> {
        specs
            .iter()
            .map(|s| format!("{}.{}", s.collection, s.name))
            .collect()
    }

    #[test]
    fn single_tenant_specs_are_the_historical_set() {
        let specs = index_specs(false);
        // 15 sync + 4 created_at + 2 invoice analytics + 3 reminders + products_key
        // + idempotency unique/ttl + serial unique + serial status.
        assert_eq!(specs.len(), 29);
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
                "product_serials_serial_number_unique"
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
        let specs = index_specs(true);
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
        let specs = index_specs(true);
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
        assert!(index_specs(false).iter().all(|s| s.collection != "tenants"));
    }

    #[test]
    fn index_names_are_unique_per_collection() {
        for multi in [false, true] {
            let mut seen = std::collections::HashSet::new();
            for s in index_specs(multi) {
                assert!(
                    seen.insert((s.collection, s.name.clone())),
                    "duplicate {}",
                    s.name
                );
            }
        }
    }
}
