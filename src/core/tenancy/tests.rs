use super::current_tenant_fn::current_tenant;
use super::scope_filter::scope_filter;
use super::scope_pipeline::scope_pipeline;
use super::stamp_document::stamp_document;
use super::tenant::Tenant;
use super::with_tenant::with_tenant;

use mongodb::bson::{Bson, doc};

fn t(id: &str) -> Tenant {
    Tenant::id(id).unwrap()
}

#[test]
fn global_scope_changes_nothing() {
    let f = doc! { "key": "a" };
    assert_eq!(scope_filter(&Tenant::Global, f.clone()), f);
    let p = vec![doc! { "$match": { "x": 1 } }];
    assert_eq!(scope_pipeline(&Tenant::Global, p.clone()).unwrap(), p);
    let d = doc! { "key": "a" };
    assert_eq!(stamp_document(&Tenant::Global, d.clone()), d);
}

#[test]
fn deny_scope_matches_only_the_quarantine_tenant() {
    assert_eq!(
        scope_filter(&Tenant::Deny, doc! {}),
        doc! { "tenant_id": "__deny__" }
    );
    let p = scope_pipeline(&Tenant::Deny, vec![doc! { "$count": "n" }]).unwrap();
    assert_eq!(p[0], doc! { "$match": { "tenant_id": "__deny__" } });
    assert_eq!(
        stamp_document(&Tenant::Deny, doc! { "k": 1 })
            .get_str("tenant_id")
            .unwrap(),
        "__deny__"
    );
}

#[tokio::test]
async fn ambient_tenant_is_visible_only_inside_the_scope() {
    assert_eq!(current_tenant(), None);
    let seen = with_tenant(t("t9"), async { current_tenant() }).await;
    assert_eq!(seen, Some(t("t9")));
    assert_eq!(current_tenant(), None);
}

#[tokio::test]
async fn ambient_tenant_does_not_follow_spawned_tasks() {
    let inside_spawn = with_tenant(t("t9"), async {
        tokio::spawn(async { current_tenant() }).await.unwrap()
    })
    .await;
    assert_eq!(inside_spawn, None);
}

#[test]
fn empty_tenant_id_is_rejected() {
    assert!(Tenant::id("  ").is_err());
}

#[test]
fn filter_is_and_wrapped_so_or_cannot_escape() {
    let scoped = scope_filter(&t("t1"), doc! { "$or": [ { "a": 1 }, { "b": 2 } ] });
    assert_eq!(
        scoped,
        doc! { "$and": [ { "tenant_id": "t1" }, { "$or": [ { "a": 1 }, { "b": 2 } ] } ] }
    );
}

#[test]
fn caller_supplied_tenant_id_cannot_override_scope() {
    let scoped = scope_filter(&t("t1"), doc! { "tenant_id": "other" });
    assert_eq!(
        scoped,
        doc! { "$and": [ { "tenant_id": "t1" }, { "tenant_id": "other" } ] }
    );
}

#[test]
fn empty_filter_becomes_plain_tenant_match() {
    assert_eq!(scope_filter(&t("t1"), doc! {}), doc! { "tenant_id": "t1" });
}

#[test]
fn stamping_overwrites_caller_tenant() {
    let d = stamp_document(&t("t1"), doc! { "key": "a", "tenant_id": "evil" });
    assert_eq!(d.get_str("tenant_id").unwrap(), "t1");
}

#[test]
fn pipeline_gets_leading_tenant_match() {
    let p = scope_pipeline(&t("t1"), vec![doc! { "$group": { "_id": "$x" } }]).unwrap();
    assert_eq!(p[0], doc! { "$match": { "tenant_id": "t1" } });
    assert_eq!(p.len(), 2);
}

#[test]
fn classic_lookup_becomes_tenant_scoped_pipeline_lookup() {
    let p = scope_pipeline(
        &t("t1"),
        vec![doc! { "$lookup": {
        "from": "payments", "localField": "key",
        "foreignField": "invoice_key", "as": "pays" } }],
    )
    .unwrap();
    let lookup = p[1].get_document("$lookup").unwrap();
    assert!(lookup.get("localField").is_none());
    assert!(lookup.get("foreignField").is_none());
    assert_eq!(lookup.get_str("from").unwrap(), "payments");
    assert_eq!(lookup.get_str("as").unwrap(), "pays");
    assert_eq!(
        lookup.get_document("let").unwrap(),
        &doc! { "tenant_lf": "$key" }
    );
    assert_eq!(
        lookup.get_array("pipeline").unwrap()[0],
        Bson::Document(doc! { "$match": {
        "tenant_id": "t1",
        "$expr": { "$eq": [ "$invoice_key", "$$tenant_lf" ] } } })
    );
}

#[test]
fn pipeline_lookup_gets_tenant_match_first_and_nested_lookups_are_scoped() {
    let p = scope_pipeline(
        &t("t1"),
        vec![doc! { "$lookup": {
        "from": "a", "as": "x",
        "pipeline": [ { "$lookup": {
            "from": "b", "localField": "k", "foreignField": "j", "as": "y" } } ] } }],
    )
    .unwrap();
    let inner = p[1]
        .get_document("$lookup")
        .unwrap()
        .get_array("pipeline")
        .unwrap();
    assert_eq!(
        inner[0],
        Bson::Document(doc! { "$match": { "tenant_id": "t1" } })
    );
    let nested = inner[1]
        .as_document()
        .unwrap()
        .get_document("$lookup")
        .unwrap();
    assert!(nested.get("localField").is_none());
    assert!(nested.get_array("pipeline").is_ok());
}

#[test]
fn lookups_inside_facet_branches_are_scoped() {
    let p = scope_pipeline(
        &t("t1"),
        vec![doc! { "$facet": {
        "a": [ { "$lookup": {
            "from": "b", "localField": "k", "foreignField": "j", "as": "y" } } ],
        "b": [ { "$count": "n" } ] } }],
    )
    .unwrap();
    let facet = p[1].get_document("$facet").unwrap();
    let a = facet.get_array("a").unwrap()[0]
        .as_document()
        .unwrap()
        .get_document("$lookup")
        .unwrap();
    assert!(a.get_array("pipeline").is_ok());
    assert!(a.get("localField").is_none());
}

#[test]
fn stages_that_cross_collections_are_rejected() {
    for stage in [
        doc! { "$unionWith": "products" },
        doc! { "$graphLookup": { "from": "x" } },
        doc! { "$out": "x" },
        doc! { "$merge": { "into": "x" } },
    ] {
        assert!(scope_pipeline(&t("t1"), vec![stage]).is_err());
    }
}

#[test]
fn lookup_without_join_spec_is_rejected() {
    let r = scope_pipeline(
        &t("t1"),
        vec![doc! { "$lookup": { "from": "x", "as": "y" } }],
    );
    assert!(r.is_err());
}
