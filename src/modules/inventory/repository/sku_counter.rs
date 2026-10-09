// Mongo access for the `sku_counters` collection — an internal
// auto-increment sequence table, not a business entity, so unlike every
// other document in this module it does *not* get a generated `key: String`
// (see CLAUDE.md's key-prefix convention). Its `_id` is itself the
// meaningful key: the derived SKU prefix code (e.g. "PHO-SCR").

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use mongodb::{bson::doc, options::ReturnDocument};
use serde::{Deserialize, Serialize};

use crate::{clients::db::Db, core::error::AppResult};

/// Mongo document shape tracking auto-increment SKU sequence per prefix.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkuCounterDocument {
    /// Derived category and subcategory prefix code (e.g. "PHO-SCR").
    #[serde(rename = "_id")]
    code: String,
    /// Last generated sequence number for this prefix.
    seq: i64,
}

fn sku_counters(db: &TenantDatabase) -> ScopedCollection<SkuCounterDocument> {
    db.collection("sku_counters")
}

/// `$inc`s the counter for `code` by `by` and returns the new value, creating it when missing.
///
/// Two requests creating the *same new* counter at once both try to insert it, and the loser
/// fails with a duplicate-key error (E11000) instead of incrementing. The counter exists by then,
/// so repeating the call increments it normally; this is the retry MongoDB documents for
/// concurrent upserts. It matters now that sample products are created concurrently.
async fn bump(db: &TenantDatabase, code: &str, by: i64) -> AppResult<i64> {
    let id = crate::modules::sequences::counter_id(db, code);
    let mut attempt = 0;
    loop {
        let result = sku_counters(db)
            .find_one_and_update(doc! { "_id": &id }, doc! { "$inc": { "seq": by } })
            .upsert(true)
            .return_document(ReturnDocument::After)
            .await;
        match result {
            Ok(updated) => {
                return Ok(updated
                    .expect("upsert guarantees find_one_and_update returns a document")
                    .seq);
            }
            Err(e) if attempt < 3 && is_duplicate_key(&e) => attempt += 1,
            Err(e) => return Err(e.into()),
        }
    }
}

fn is_duplicate_key(err: &mongodb::error::Error) -> bool {
    use mongodb::error::{ErrorKind, WriteFailure};
    match err.kind.as_ref() {
        ErrorKind::Command(c) => c.code == 11000,
        ErrorKind::Write(WriteFailure::WriteError(w)) => w.code == 11000,
        _ => false,
    }
}

/// Atomically increments and returns the next sequence number for `code`,
/// creating the counter starting at 1 if it doesn't exist yet.
pub(crate) async fn next_sequence(db: &Db, code: &str) -> AppResult<i64> {
    match db {
        Db::Mongo(db) => bump(db, code, 1).await,
        Db::Sqlite(pool) => {
            let seq: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO sku_counters (prefix, seq) VALUES (?, 1)
                ON CONFLICT(prefix) DO UPDATE SET seq = seq + 1
                RETURNING seq
                "#,
            )
            .bind(code)
            .fetch_one(pool)
            .await?;

            Ok(seq)
        }
    }
}

/// Atomically reserves a contiguous block of `block_size` sequence numbers
/// for `code`, drawn from the exact same counter row `next_sequence`
/// increments one at a time — a block a linked device pre-fetches and a SKU
/// generated directly by `generate_sku` (unlinked, or online with no block
/// held) can therefore never land on the same number.
pub(crate) async fn reserve_block(db: &Db, code: &str, block_size: i64) -> AppResult<(i64, i64)> {
    match db {
        Db::Mongo(db) => {
            let seq = bump(db, code, block_size).await?;
            Ok((seq - block_size + 1, seq))
        }
        Db::Sqlite(pool) => {
            let seq: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO sku_counters (prefix, seq) VALUES (?, ?)
                ON CONFLICT(prefix) DO UPDATE SET seq = seq + ?
                RETURNING seq
                "#,
            )
            .bind(code)
            .bind(block_size)
            .bind(block_size)
            .fetch_one(pool)
            .await?;
            Ok((seq - block_size + 1, seq))
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
                    next_sequence(&db, "PHO-SCR").await
                })
                .await;
                assert_eq!(n.unwrap(), round, "{t} round {round}");
            }
        }
        raw.drop().await.ok();
    }

    // Atlas-backed (MONGODB_URI); skipped when unset.
    #[tokio::test]
    async fn concurrent_first_use_of_a_prefix_never_fails_or_repeats_a_number() {
        dotenvy::dotenv().ok();
        let Ok(uri) = std::env::var("MONGODB_URI") else {
            eprintln!("MONGODB_URI not set - skipping");
            return;
        };
        let name = format!("jtcnt_{}", &uuid::Uuid::new_v4().simple().to_string()[..24]);
        let raw = crate::clients::mongo::connect(&uri, &name).await.unwrap();
        let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));

        let mut numbers = with_tenant(Tenant::id("shop_a").unwrap(), async {
            let calls = (0..16).map(|_| next_sequence(&db, "NEW-PFX"));
            futures_util::future::try_join_all(calls).await.unwrap()
        })
        .await;
        numbers.sort_unstable();
        assert_eq!(numbers, (1..=16).collect::<Vec<i64>>());
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
            // A directly-generated SKU takes 1.
            assert_eq!(next_sequence(&db, "PHO-SCR").await.unwrap(), 1);
            // A device pre-fetches a block of 5: must start right after.
            let (start, end) = reserve_block(&db, "PHO-SCR", 5).await.unwrap();
            assert_eq!((start, end), (2, 6));
            // Another direct generation (e.g. an unlinked second device, or the
            // cloud itself creating a product) must land AFTER the reserved
            // block, never inside it.
            assert_eq!(next_sequence(&db, "PHO-SCR").await.unwrap(), 7);
        })
        .await;
        raw.drop().await.ok();
    }
}
