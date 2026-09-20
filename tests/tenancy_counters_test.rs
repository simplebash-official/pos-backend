// Counters and sequence blocks under multi-tenant scoping, against a real
// MongoDB (MONGODB_URI) in a throwaway database that is dropped afterwards.
// Two tenants must count independently starting from the same value, and the
// single-shop handle must keep the bare `_id` it always used.

use mongodb::bson::{Document, doc};
use simplebash_pos_backend::{
    clients::{db::Db, mongo::connect, tenant_db::TenantDatabase},
    core::tenancy::{Tenant, with_tenant},
    domain::sequences::ReserveSequenceRequest,
    modules::sequences::service::reserve_sequence,
};
use uuid::Uuid;

fn request(size: u64) -> ReserveSequenceRequest {
    ReserveSequenceRequest {
        block_size: Some(size),
        device_id: Some("dev_1".into()),
    }
}

async fn throwaway() -> Option<(mongodb::Database, String)> {
    dotenvy::dotenv().ok();
    let uri = std::env::var("MONGODB_URI").ok()?;
    let name = format!("jtcnt_{}", &Uuid::new_v4().simple().to_string()[..24]);
    Some((connect(&uri, &name).await.expect("connect"), name))
}

#[tokio::test]
async fn sequence_blocks_are_independent_per_tenant() {
    let Some((raw, _)) = throwaway().await else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };
    let db = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));
    let tenant = |id: &str| Tenant::id(id).unwrap();

    // Both tenants reserve "invoice": each starts at 1, and continues on its own.
    let mut seen = Vec::new();
    for _ in 0..2 {
        for t in ["shop_a", "shop_b"] {
            let r = with_tenant(tenant(t), reserve_sequence(&db, "invoice".into(), request(5)))
                .await
                .unwrap();
            seen.push((t, r.start, r.end));
        }
    }
    assert_eq!(
        seen,
        vec![
            ("shop_a", 1, 5),
            ("shop_b", 1, 5),
            ("shop_a", 6, 10),
            ("shop_b", 6, 10),
        ]
    );

    // Stored counter ids are tenant-qualified, one document per tenant.
    let counters = raw.collection::<Document>("sequence_counters");
    assert_eq!(counters.count_documents(doc! {}).await.unwrap(), 2);
    assert_eq!(
        counters
            .count_documents(doc! { "_id": "shop_a:invoice", "tenant_id": "shop_a" })
            .await
            .unwrap(),
        1
    );

    // Outside any tenant scope nothing is reachable and nothing real is touched.
    let denied = reserve_sequence(&db, "invoice".into(), request(1)).await;
    let _ = denied; // quarantined under the deny tenant, never a real tenant
    assert_eq!(
        counters
            .count_documents(doc! { "_id": "shop_a:invoice" })
            .await
            .unwrap(),
        1
    );

    raw.drop().await.ok();
}

#[tokio::test]
async fn single_shop_handle_keeps_bare_counter_ids() {
    let Some((raw, _)) = throwaway().await else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };
    let db = Db::from_mongo(raw.clone());

    let r = reserve_sequence(&db, "invoice".into(), request(3)).await.unwrap();
    assert_eq!((r.start, r.end), (1, 3));
    let counters = raw.collection::<Document>("sequence_counters");
    assert_eq!(
        counters.count_documents(doc! { "_id": "invoice" }).await.unwrap(),
        1,
        "single-shop deployments keep the bare _id"
    );
    let stored = counters.find_one(doc! { "_id": "invoice" }).await.unwrap().unwrap();
    assert!(stored.get("tenant_id").is_none(), "no tenant field in single-shop mode");

    raw.drop().await.ok();
}
