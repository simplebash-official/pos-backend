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
    "repairs",
    "print_jobs",
    "invoices",
    "payments",
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

    tracing::info!("Database indexes ensured");
}
