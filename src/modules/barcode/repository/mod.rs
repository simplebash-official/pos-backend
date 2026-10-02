// Mongo access for the `barcode_counters` collection only — one document
// per namespace (e.g. `"product"`). Visibility is `pub(crate)` so `service`
// can call in, but the `mod repository;` declaration in `barcode/mod.rs` is
// private, so none of this is reachable from outside this module tree.

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use mongodb::{bson::doc, options::ReturnDocument};

use crate::{
    clients::db::Db, core::error::AppResult, modules::barcode::model::BarcodeCounterDocument,
};

fn barcode_counters(db: &TenantDatabase) -> ScopedCollection<BarcodeCounterDocument> {
    db.collection("barcode_counters")
}

/// Atomically increments and returns the next sequence number for
/// `namespace`, creating the counter starting at 1 if it doesn't exist yet
/// — the same `$inc` + `upsert` pattern as `inventory::repository::sku_counter::next_sequence`,
/// safe under concurrent generation requests without a uniqueness-retry loop.
pub(crate) async fn next_sequence(db: &Db, namespace: &str) -> AppResult<i64> {
    match db {
        Db::Mongo(db) => {
            let updated = barcode_counters(db)
                .find_one_and_update(
                    doc! { "_id": crate::modules::sequences::counter_id(db, namespace) },
                    doc! { "$inc": { "seq": 1i64 } },
                )
                .upsert(true)
                .return_document(ReturnDocument::After)
                .await?;

            Ok(updated
                .expect("upsert guarantees find_one_and_update returns a document")
                .seq)
        }
        Db::Sqlite(pool) => {
            let row: (i64,) = sqlx::query_as(
                r#"
                INSERT INTO barcode_counters (prefix, seq)
                VALUES ($1, 1)
                ON CONFLICT(prefix) DO UPDATE SET
                    seq = barcode_counters.seq + 1
                RETURNING seq
                "#,
            )
            .bind(namespace)
            .fetch_one(pool)
            .await?;
            Ok(row.0)
        }
    }
}

/// Atomically reserves a contiguous block of `block_size` sequence numbers
/// for `namespace`, drawn from the exact same counter row `next_sequence`
/// increments one at a time — a block a linked device pre-fetches and a
/// barcode generated directly (unlinked, or online with no block held) can
/// therefore never land on the same number.
pub(crate) async fn reserve_block(
    db: &Db,
    namespace: &str,
    block_size: i64,
) -> AppResult<(i64, i64)> {
    match db {
        Db::Mongo(db) => {
            let updated = barcode_counters(db)
                .find_one_and_update(
                    doc! { "_id": crate::modules::sequences::counter_id(db, namespace) },
                    doc! { "$inc": { "seq": block_size } },
                )
                .upsert(true)
                .return_document(ReturnDocument::After)
                .await?;

            let seq = updated
                .expect("upsert guarantees find_one_and_update returns a document")
                .seq;
            Ok((seq - block_size + 1, seq))
        }
        Db::Sqlite(pool) => {
            let row: (i64,) = sqlx::query_as(
                r#"
                INSERT INTO barcode_counters (prefix, seq)
                VALUES (?, ?)
                ON CONFLICT(prefix) DO UPDATE SET
                    seq = barcode_counters.seq + ?
                RETURNING seq
                "#,
            )
            .bind(namespace)
            .bind(block_size)
            .bind(block_size)
            .fetch_one(pool)
            .await?;
            Ok((row.0 - block_size + 1, row.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clients::tenant_db::TenantDatabase,
        core::tenancy::{Tenant, with_tenant},
    };

    // Atlas-backed (MONGODB_URI); skipped when unset.
    #[tokio::test]
    async fn counters_are_independent_per_tenant() {
        dotenvy::dotenv().ok();
        let Ok(uri) = std::env::var("MONGODB_URI") else {
            eprintln!("MONGODB_URI not set - skipping");
            return;
        };
        let name = format!("jtcnt_{}", &uuid::Uuid::new_v4().simple().to_string()[..24]);
        let raw = crate::clients::mongo::connect(&uri, &name).await.unwrap();
        let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));

        // Interleave the tenants: each must count 1, 2, 3 on its own.
        for round in 1..=3i64 {
            for t in ["shop_a", "shop_b"] {
                let n = with_tenant(Tenant::id(t).unwrap(), async {
                    next_sequence(&db, "product").await
                })
                .await;
                assert_eq!(n.unwrap(), round, "{t} round {round}");
            }
        }
        raw.drop().await.ok();
    }

    // Atlas-backed (MONGODB_URI); skipped when unset.
    #[tokio::test]
    async fn a_reserved_block_and_a_direct_sequence_never_overlap() {
        dotenvy::dotenv().ok();
        let Ok(uri) = std::env::var("MONGODB_URI") else {
            eprintln!("MONGODB_URI not set - skipping");
            return;
        };
        let name = format!("jtcnt_{}", &uuid::Uuid::new_v4().simple().to_string()[..24]);
        let raw = crate::clients::mongo::connect(&uri, &name).await.unwrap();
        let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));

        with_tenant(Tenant::id("shop_a").unwrap(), async {
            assert_eq!(next_sequence(&db, "product").await.unwrap(), 1);
            let (start, end) = reserve_block(&db, "product", 5).await.unwrap();
            assert_eq!((start, end), (2, 6));
            assert_eq!(next_sequence(&db, "product").await.unwrap(), 7);
        })
        .await;
        raw.drop().await.ok();
    }
}
