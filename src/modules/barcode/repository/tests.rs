use crate::clients::db::Db;

use super::*;
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
