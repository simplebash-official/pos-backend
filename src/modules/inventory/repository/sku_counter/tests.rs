use super::next_sequence::next_sequence;
use super::reserve_block::reserve_block;
use crate::clients::db::Db;

use crate::clients::tenant_db::TenantDatabase;
use crate::core::tenancy::{Tenant, with_tenant};

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
