// Isolation guarantees of the tenant-scoped MongoDB handle, run against a real
// MongoDB (MONGODB_URI from .env / the environment) in a uniquely named
// throwaway database that is dropped afterwards. The tests deliberately reuse
// the SAME `key` values in both tenants, so a missing tenant condition shows up
// as a wrong count or a leaked row instead of passing by luck.

use mongodb::bson::{Document, doc};
use simplebash_pos_backend::{
    clients::{db::Db, mongo::connect, tenant_db::TenantDatabase},
    core::tenancy::{Tenant, with_tenant},
};
use uuid::Uuid;

struct Fixture {
    global: Db,
    a: Db,
    b: Db,
}

impl Fixture {
    async fn new() -> Option<Fixture> {
        dotenvy::dotenv().ok();
        let uri = std::env::var("MONGODB_URI").ok()?;
        let name = format!("jtten_{}", &Uuid::new_v4().simple().to_string()[..24]);
        let raw = connect(&uri, &name).await.expect("connect throwaway db");
        let global = Db::from_mongo(raw);
        Some(Fixture {
            a: global.for_tenant(Tenant::id("tenant_a").unwrap()),
            b: global.for_tenant(Tenant::id("tenant_b").unwrap()),
            global,
        })
    }

    async fn cleanup(self) {
        self.global.as_mongo().unwrap().unscoped().drop().await.ok();
    }
}

fn coll(
    db: &Db,
    name: &str,
) -> simplebash_pos_backend::clients::tenant_db::ScopedCollection<Document> {
    db.as_mongo().unwrap().collection::<Document>(name)
}

#[tokio::test]
async fn reads_writes_and_deletes_stay_inside_the_tenant() {
    let Some(f) = Fixture::new().await else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };

    for (db, qty) in [(&f.a, 1), (&f.b, 100)] {
        coll(db, "items")
            .insert_one(doc! { "key": "item_1", "qty": qty })
            .await
            .unwrap();
        coll(db, "items")
            .insert_one(doc! { "key": "item_2", "qty": qty })
            .await
            .unwrap();
    }

    // Same keys exist in both tenants; each sees only its own.
    assert_eq!(
        coll(&f.a, "items").count_documents(doc! {}).await.unwrap(),
        2
    );
    assert_eq!(
        coll(&f.b, "items").count_documents(doc! {}).await.unwrap(),
        2
    );
    assert_eq!(
        coll(&f.global, "items")
            .count_documents(doc! {})
            .await
            .unwrap(),
        4
    );

    let a_item = coll(&f.a, "items")
        .find_one(doc! { "key": "item_1" })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a_item.get_i32("qty").unwrap(), 1);
    assert_eq!(a_item.get_str("tenant_id").unwrap(), "tenant_a");

    // An update by tenant A never touches tenant B's identically keyed row.
    coll(&f.a, "items")
        .update_many(doc! { "key": "item_1" }, doc! { "$set": { "qty": 7 } })
        .await
        .unwrap();
    let b_item = coll(&f.b, "items")
        .find_one(doc! { "key": "item_1" })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b_item.get_i32("qty").unwrap(), 100);

    // find_one_and_update cannot reach across tenants either.
    let miss = coll(&f.a, "items")
        .find_one_and_update(
            doc! { "key": "item_1", "qty": 100 },
            doc! { "$set": { "qty": 0 } },
        )
        .await
        .unwrap();
    assert!(miss.is_none());

    // A filter that names another tenant explicitly matches nothing.
    let mut forged = coll(&f.a, "items")
        .find(doc! { "tenant_id": "tenant_b" })
        .await
        .unwrap();
    assert!(!forged.advance().await.unwrap());

    // Deletes are scoped as well.
    coll(&f.a, "items").delete_many(doc! {}).await.unwrap();
    assert_eq!(
        coll(&f.a, "items").count_documents(doc! {}).await.unwrap(),
        0
    );
    assert_eq!(
        coll(&f.b, "items").count_documents(doc! {}).await.unwrap(),
        2
    );

    f.cleanup().await;
}

#[tokio::test]
async fn upsert_and_insert_cannot_escape_the_tenant() {
    let Some(f) = Fixture::new().await else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };

    // A caller-supplied tenant_id on insert is overwritten with the scope.
    coll(&f.a, "docs")
        .insert_one(doc! { "key": "d1", "tenant_id": "tenant_b" })
        .await
        .unwrap();
    assert_eq!(
        coll(&f.b, "docs").count_documents(doc! {}).await.unwrap(),
        0
    );
    assert_eq!(
        coll(&f.a, "docs").count_documents(doc! {}).await.unwrap(),
        1
    );

    // An upsert creates the document inside the calling tenant.
    coll(&f.b, "counters")
        .find_one_and_update(doc! { "_name": "seq" }, doc! { "$inc": { "n": 1 } })
        .upsert(true)
        .await
        .unwrap();
    let created = coll(&f.global, "counters")
        .find_one(doc! { "_name": "seq" })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created.get_str("tenant_id").unwrap(), "tenant_b");
    assert_eq!(
        coll(&f.a, "counters")
            .count_documents(doc! {})
            .await
            .unwrap(),
        0
    );

    f.cleanup().await;
}

#[tokio::test]
async fn aggregation_lookups_and_facets_are_tenant_scoped() {
    let Some(f) = Fixture::new().await else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };

    // Both tenants own an invoice "inv_1" with payments; A has 1, B has 3.
    for (db, payments) in [(&f.a, 1), (&f.b, 3)] {
        coll(db, "invoices")
            .insert_one(doc! { "key": "inv_1", "total": 10 })
            .await
            .unwrap();
        for i in 0..payments {
            coll(db, "payments")
                .insert_one(doc! { "key": format!("pay_{i}"), "invoice_key": "inv_1", "amount": 5 })
                .await
                .unwrap();
        }
    }

    // Classic localField/foreignField lookup: must not see the other tenant's payments.
    let pipeline = vec![doc! { "$lookup": {
    "from": "payments", "localField": "key",
    "foreignField": "invoice_key", "as": "pays" } }];
    let mut cursor = coll(&f.a, "invoices")
        .aggregate(pipeline.clone())
        .await
        .unwrap();
    assert!(cursor.advance().await.unwrap());
    let row = cursor.deserialize_current().unwrap();
    assert_eq!(row.get_array("pays").unwrap().len(), 1);
    assert!(
        !cursor.advance().await.unwrap(),
        "only tenant A's invoice is returned"
    );

    let mut cursor = coll(&f.b, "invoices").aggregate(pipeline).await.unwrap();
    assert!(cursor.advance().await.unwrap());
    assert_eq!(
        cursor
            .deserialize_current()
            .unwrap()
            .get_array("pays")
            .unwrap()
            .len(),
        3
    );

    // The same lookup inside a $facet branch is scoped too.
    let faceted = vec![doc! { "$facet": {
    "joined": [ { "$lookup": {
        "from": "payments", "localField": "key",
        "foreignField": "invoice_key", "as": "pays" } } ],
    "n": [ { "$count": "n" } ] } }];
    let mut cursor = coll(&f.a, "invoices").aggregate(faceted).await.unwrap();
    assert!(cursor.advance().await.unwrap());
    let row = cursor.deserialize_current().unwrap();
    let joined = row.get_array("joined").unwrap();
    assert_eq!(joined.len(), 1);
    assert_eq!(
        joined[0]
            .as_document()
            .unwrap()
            .get_array("pays")
            .unwrap()
            .len(),
        1
    );

    // Stages that could read another collection unscoped are refused outright.
    let err = coll(&f.a, "invoices")
        .aggregate(vec![doc! { "$unionWith": "payments" }])
        .await;
    assert!(err.is_err());

    f.cleanup().await;
}

#[tokio::test]
async fn ambient_tenant_scopes_a_shared_multi_tenant_handle_and_fails_closed() {
    dotenvy::dotenv().ok();
    let Ok(uri) = std::env::var("MONGODB_URI") else {
        eprintln!("MONGODB_URI not set - skipping");
        return;
    };
    let name = format!("jtten_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let raw = connect(&uri, &name).await.expect("connect throwaway db");
    // One handle, like AppState.db: the tenant comes from the running request.
    let shared = Db::Mongo(TenantDatabase::multi_tenant(raw.clone()));
    let tenant = |id: &str| Tenant::id(id).unwrap();

    with_tenant(tenant("shop_a"), async {
        coll(&shared, "items")
            .insert_one(doc! { "key": "k1", "owner": "a" })
            .await
            .unwrap();
    })
    .await;
    with_tenant(tenant("shop_b"), async {
        coll(&shared, "items")
            .insert_one(doc! { "key": "k1", "owner": "b" })
            .await
            .unwrap();
    })
    .await;

    let owner_seen_by = |id: &'static str| {
        let shared = shared.clone();
        async move {
            with_tenant(tenant(id), async {
                let row = coll(&shared, "items")
                    .find_one(doc! { "key": "k1" })
                    .await
                    .unwrap()
                    .unwrap();
                row.get_str("owner").unwrap().to_string()
            })
            .await
        }
    };
    assert_eq!(owner_seen_by("shop_a").await, "a");
    assert_eq!(owner_seen_by("shop_b").await, "b");

    // No tenant in scope (unauthenticated / forgotten scope): nothing is visible...
    assert_eq!(
        coll(&shared, "items")
            .count_documents(doc! {})
            .await
            .unwrap(),
        0
    );
    // ...and a stray write is quarantined, never placed in a real tenant.
    coll(&shared, "items")
        .insert_one(doc! { "key": "stray" })
        .await
        .unwrap();
    for id in ["shop_a", "shop_b"] {
        let n = with_tenant(tenant(id), async {
            coll(&shared, "items")
                .count_documents(doc! { "key": "stray" })
                .await
                .unwrap()
        })
        .await;
        assert_eq!(n, 0, "stray write leaked into {id}");
    }
    let quarantined = raw
        .collection::<Document>("items")
        .count_documents(doc! { "tenant_id": "__deny__" })
        .await
        .unwrap();
    assert_eq!(quarantined, 1);

    raw.drop().await.ok();
}
