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

use mongodb::{Database, IndexModel, bson::doc, options::IndexOptions};
use std::time::Duration;

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

/// Best-effort: a failure here is logged and the server still starts. An
/// index that could not be created makes sync slower, not wrong, and
/// refusing to boot over it would take the shop offline for a problem that
/// resolves itself on the next deploy.
pub async fn ensure_indexes(db: &Database) {
    for collection_name in SYNCED_COLLECTIONS {
        let model = IndexModel::builder()
            .keys(doc! { "updated_at": 1, "key": 1 })
            .options(
                IndexOptions::builder()
                    .name("sync_updated_at_key".to_string())
                    .build(),
            )
            .build();

        if let Err(err) = db
            .collection::<mongodb::bson::Document>(collection_name)
            .create_index(model)
            .await
        {
            tracing::warn!(
                collection = collection_name,
                %err,
                "could not create sync cursor index"
            );
        }
    }

    // Analytics reporting (`modules::reports::repository::analytics`) matches
    // and buckets on `created_at` across whole collections. Without these the
    // POS has no `created_at` index at all and every analytics range query is
    // a full collection scan — the single biggest lever on report latency as
    // a shop's history grows. Best-effort, same as the sync indexes above.
    for collection_name in ["invoices", "credit_notes", "purchases", "payments"] {
        let model = IndexModel::builder()
            .keys(doc! { "created_at": 1 })
            .options(
                IndexOptions::builder()
                    .name("analytics_created_at".to_string())
                    .build(),
            )
            .build();

        if let Err(err) = db
            .collection::<mongodb::bson::Document>(collection_name)
            .create_index(model)
            .await
        {
            tracing::warn!(collection = collection_name, %err, "could not create analytics created_at index");
        }
    }

    // Per-cashier and per-customer analytics slices scan `invoices` filtered
    // by one id and a date range — a compound index keeps those bounded.
    for (name, keys) in [
        (
            "analytics_cashier_created_at",
            doc! { "cashier_id": 1, "created_at": 1 },
        ),
        (
            "analytics_customer_created_at",
            doc! { "customer_key": 1, "created_at": 1 },
        ),
    ] {
        let model = IndexModel::builder()
            .keys(keys)
            .options(IndexOptions::builder().name(name.to_string()).build())
            .build();

        if let Err(err) = db
            .collection::<mongodb::bson::Document>("invoices")
            .create_index(model)
            .await
        {
            tracing::warn!(index = name, %err, "could not create invoices analytics compound index");
        }
    }

    // Credit-reminder / receivables reads filter `invoices` by lifecycle
    // status and sort/filter by `due_date`; repair & print-job reminders do
    // the same over `promised_ready_at` + status. Best-effort compound
    // indexes keep those scans bounded.
    for (collection_name, name, keys) in [
        (
            "invoices",
            "reminders_status_due_date",
            doc! { "status": 1, "due_date": 1 },
        ),
        (
            "repairs",
            "reminders_promised_ready_at_status",
            doc! { "promised_ready_at": 1, "status": 1 },
        ),
        (
            "print_jobs",
            "reminders_promised_ready_at_status",
            doc! { "promised_ready_at": 1, "status": 1 },
        ),
    ] {
        let model = IndexModel::builder()
            .keys(keys)
            .options(IndexOptions::builder().name(name.to_string()).build())
            .build();

        if let Err(err) = db
            .collection::<mongodb::bson::Document>(collection_name)
            .create_index(model)
            .await
        {
            tracing::warn!(collection = collection_name, index = name, %err, "could not create reminders index");
        }
    }

    // `sales-by-category` joins invoice lines to `products` by `key`; the sync
    // index has `key` second, so a `$lookup` on it alone can't use it.
    {
        let model = IndexModel::builder()
            .keys(doc! { "key": 1 })
            .options(
                IndexOptions::builder()
                    .name("products_key".to_string())
                    .build(),
            )
            .build();

        if let Err(err) = db
            .collection::<mongodb::bson::Document>("products")
            .create_index(model)
            .await
        {
            tracing::warn!(%err, "could not create products key index");
        }
    }

    let idempotency = db.collection::<mongodb::bson::Document>("idempotency_keys");

    let unique_key = IndexModel::builder()
        .keys(doc! { "key": 1, "user_id": 1 })
        .options(
            IndexOptions::builder()
                .name("idempotency_key_user_unique".to_string())
                .unique(true)
                .build(),
        )
        .build();

    if let Err(err) = idempotency.create_index(unique_key).await {
        tracing::warn!(%err, "could not create unique idempotency key index");
    }

    let ttl = IndexModel::builder()
        .keys(doc! { "created_at": 1 })
        .options(
            IndexOptions::builder()
                .name("idempotency_ttl".to_string())
                .expire_after(IDEMPOTENCY_RETENTION)
                .build(),
        )
        .build();

    if let Err(err) = idempotency.create_index(ttl).await {
        tracing::warn!(%err, "could not create idempotency TTL index");
    }

    let product_serials = db.collection::<mongodb::bson::Document>("product_serials");

    let unique_serial_number = IndexModel::builder()
        .keys(doc! { "serial_number": 1 })
        .options(
            IndexOptions::builder()
                .name("product_serials_serial_number_unique".to_string())
                .unique(true)
                .build(),
        )
        .build();

    if let Err(err) = product_serials.create_index(unique_serial_number).await {
        tracing::warn!(%err, "could not create unique product serial number index");
    }

    // Backs `GET /products/{key}/serials?status=` — a product's serial list
    // filtered by lifecycle status (e.g. the sale-time `in_stock` picker).
    let product_status = IndexModel::builder()
        .keys(doc! { "product_key": 1, "status": 1 })
        .options(
            IndexOptions::builder()
                .name("product_serials_product_key_status".to_string())
                .build(),
        )
        .build();

    if let Err(err) = product_serials.create_index(product_status).await {
        tracing::warn!(%err, "could not create product serials product/status index");
    }

    tracing::info!("Database indexes ensured");
}
