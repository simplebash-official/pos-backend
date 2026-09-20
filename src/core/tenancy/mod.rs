// Tenant isolation core: the `Tenant` scope a database handle carries and the
// pure rewrites (filters, aggregation pipelines, inserted documents) that make
// every MongoDB read and write stay inside one tenant. Nothing here does I/O;
// `clients::tenant_db` applies these rewrites to the real driver calls so a
// repository cannot forget the tenant filter.

use std::{future::Future, sync::Arc};

use mongodb::bson::{Bson, Document, doc};

use crate::core::error::{AppError, AppResult};

/// Field carried by every tenant-owned MongoDB document.
pub const TENANT_FIELD: &str = "tenant_id";

/// Tenant value used by the fail-closed scope. Nothing legitimate ever has this
/// id, so reads match nothing; a write that slips through lands here, visibly
/// quarantined, instead of in a real tenant.
pub const DENY_TENANT: &str = "__deny__";

tokio::task_local! {
    static CURRENT_TENANT: Tenant;
}

/// Runs `fut` with `tenant` as the ambient tenant of every multi-tenant
/// database handle opened inside it (the request middleware wraps each
/// authenticated request this way). The value does not follow a `tokio::spawn`,
/// which is deliberate: work that escapes the request scope fails closed.
pub async fn with_tenant<F: Future>(tenant: Tenant, fut: F) -> F::Output {
    CURRENT_TENANT.scope(tenant, fut).await
}

/// Id of the ambient tenant, if the running request is confined to one.
pub fn current_tenant_id() -> Option<String> {
    match current_tenant() {
        Some(Tenant::Id(id)) => Some(id.to_string()),
        _ => None,
    }
}

/// The ambient tenant of the running request, if one was set.
pub fn current_tenant() -> Option<Tenant> {
    CURRENT_TENANT.try_with(Clone::clone).ok()
}

/// Which data a database handle may see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tenant {
    /// No tenant filtering: single-shop deployments (desktop, self-hosted) and
    /// platform-level collections. Behaves exactly like the raw driver.
    Global,
    /// Multi-tenant cloud: every operation is confined to this tenant id.
    Id(Arc<str>),
    /// Multi-tenant cloud, no tenant known (unauthenticated or out-of-request
    /// work): matches nothing, so a missed scope returns no data.
    Deny,
}

impl Tenant {
    pub fn id(id: impl AsRef<str>) -> AppResult<Tenant> {
        let id = id.as_ref().trim();
        if id.is_empty() {
            return Err(AppError::internal("tenant id must not be empty"));
        }
        Ok(Tenant::Id(Arc::from(id)))
    }

    pub fn is_scoped(&self) -> bool {
        !matches!(self, Tenant::Global)
    }

    fn value(&self) -> Option<Bson> {
        match self {
            Tenant::Global => None,
            Tenant::Id(id) => Some(Bson::String(id.to_string())),
            Tenant::Deny => Some(Bson::String(DENY_TENANT.to_string())),
        }
    }
}

/// Confines a query filter to the tenant. The original filter is nested under
/// `$and` so nothing inside it (an `$or`, or an explicit `tenant_id` key from a
/// caller) can widen the tenant condition.
pub fn scope_filter(tenant: &Tenant, filter: Document) -> Document {
    match tenant.value() {
        None => filter,
        Some(t) if filter.is_empty() => doc! { TENANT_FIELD: t },
        Some(t) => doc! { "$and": [ { TENANT_FIELD: t }, filter ] },
    }
}

/// Adds the tenant to a document about to be inserted, overwriting any
/// `tenant_id` the caller supplied.
pub fn stamp_document(tenant: &Tenant, mut document: Document) -> Document {
    if let Some(t) = tenant.value() {
        document.insert(TENANT_FIELD, t);
    }
    document
}

/// Confines an aggregation pipeline to the tenant: a leading `$match`, and
/// every nested collection read (`$lookup`, including inside `$facet`) is
/// rewritten to the tenant as well. Stages that could read or write another
/// collection without a tenant condition are rejected instead of guessed at.
pub fn scope_pipeline(tenant: &Tenant, pipeline: Vec<Document>) -> AppResult<Vec<Document>> {
    let Some(t) = tenant.value() else {
        return Ok(pipeline);
    };
    let mut scoped = Vec::with_capacity(pipeline.len() + 1);
    scoped.push(doc! { "$match": { TENANT_FIELD: t.clone() } });
    for stage in pipeline {
        scoped.push(scope_stage(&t, stage)?);
    }
    Ok(scoped)
}

fn scope_stage(tenant: &Bson, stage: Document) -> AppResult<Document> {
    let mut stage = stage;
    if let Some(name) = stage
        .keys()
        .find(|k| matches!(k.as_str(), "$unionWith" | "$graphLookup" | "$merge" | "$out"))
    {
        return Err(AppError::internal(format!(
            "aggregation stage {name} is not allowed on a tenant-scoped collection"
        )));
    }

    if let Ok(lookup) = stage.get_document("$lookup") {
        let rewritten = scope_lookup(tenant, lookup.clone())?;
        stage.insert("$lookup", rewritten);
    }

    if let Ok(facets) = stage.get_document("$facet") {
        let mut scoped_facets = Document::new();
        for (name, branch) in facets {
            let Bson::Array(stages) = branch else {
                return Err(AppError::internal("$facet branch must be a pipeline array"));
            };
            let mut out = Vec::with_capacity(stages.len());
            for s in stages {
                let Bson::Document(s) = s else {
                    return Err(AppError::internal("pipeline stage must be a document"));
                };
                out.push(Bson::Document(scope_stage(tenant, s.clone())?));
            }
            scoped_facets.insert(name.clone(), Bson::Array(out));
        }
        stage.insert("$facet", scoped_facets);
    }
    Ok(stage)
}

/// Rewrites a `$lookup` so the joined collection is filtered to the tenant.
/// The classic `localField`/`foreignField` form becomes the equivalent
/// `let`/`pipeline` form (`$eq` keeps the foreign-field index usable).
fn scope_lookup(tenant: &Bson, lookup: Document) -> AppResult<Document> {
    let mut lookup = lookup;
    if let Ok(pipeline) = lookup.get_array("pipeline") {
        let mut inner = vec![Bson::Document(doc! { "$match": { TENANT_FIELD: tenant.clone() } })];
        for s in pipeline {
            let Bson::Document(s) = s else {
                return Err(AppError::internal("pipeline stage must be a document"));
            };
            inner.push(Bson::Document(scope_stage(tenant, s.clone())?));
        }
        lookup.insert("pipeline", Bson::Array(inner));
        return Ok(lookup);
    }

    let local = lookup
        .get_str("localField")
        .map_err(|_| AppError::internal("$lookup needs localField/foreignField or a pipeline"))?
        .to_string();
    let foreign = lookup
        .get_str("foreignField")
        .map_err(|_| AppError::internal("$lookup needs localField/foreignField or a pipeline"))?
        .to_string();
    lookup.remove("localField");
    lookup.remove("foreignField");

    let mut vars = lookup.get_document("let").cloned().unwrap_or_default();
    vars.insert("tenant_lf", format!("${local}"));
    lookup.insert("let", vars);
    lookup.insert(
        "pipeline",
        vec![doc! {
            "$match": {
                TENANT_FIELD: tenant.clone(),
                "$expr": { "$eq": [ format!("${foreign}"), "$$tenant_lf" ] },
            }
        }],
    );
    Ok(lookup)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            stamp_document(&Tenant::Deny, doc! { "k": 1 }).get_str("tenant_id").unwrap(),
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
        let nested = inner[1].as_document().unwrap().get_document("$lookup").unwrap();
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
        let r = scope_pipeline(&t("t1"), vec![doc! { "$lookup": { "from": "x", "as": "y" } }]);
        assert!(r.is_err());
    }
}
