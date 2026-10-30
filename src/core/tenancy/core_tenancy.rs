use crate::core::error::{AppError, AppResult};
use mongodb::bson::{Bson, Document, doc};
use std::future::Future;
use std::sync::Arc;

tokio::task_local! {
    pub(super) static CURRENT_TENANT: Tenant;
}

/// The ambient tenant of the running request, if one was set.
pub fn current_tenant() -> Option<Tenant> {
    CURRENT_TENANT.try_with(Clone::clone).ok()
}

/// Id of the ambient tenant, if the running request is confined to one.
#[must_use]
pub fn current_tenant_id() -> Option<String> {
    match current_tenant() {
        Some(Tenant::Id(id)) => Some(id.to_string()),
        _ => None,
    }
}

/// Tenant value used by the fail-closed scope. Nothing legitimate ever has this
/// id, so reads match nothing; a write that slips through lands here, visibly
/// quarantined, instead of in a real tenant.
pub const DENY_TENANT: &str = "__deny__";

/// Confines a query filter to the tenant. The original filter is nested under
/// `$and` so nothing inside it (an `$or`, or an explicit `tenant_id` key from a
/// caller) can widen the tenant condition.
#[must_use]
pub fn scope_filter(tenant: &Tenant, filter: Document) -> Document {
    match tenant.value() {
        None => filter,
        Some(t) if filter.is_empty() => doc! { TENANT_FIELD: t },
        Some(t) => doc! { "$and": [ { TENANT_FIELD: t }, filter ] },
    }
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
    if let Some(name) = stage.keys().find(|k| {
        matches!(
            k.as_str(),
            "$unionWith" | "$graphLookup" | "$merge" | "$out"
        )
    }) {
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
        let mut inner = vec![Bson::Document(
            doc! { "$match": { TENANT_FIELD: tenant.clone() } },
        )];
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

/// Adds the tenant to a document about to be inserted, overwriting any
/// `tenant_id` the caller supplied.
#[must_use]
pub fn stamp_document(tenant: &Tenant, mut document: Document) -> Document {
    if let Some(t) = tenant.value() {
        document.insert(TENANT_FIELD, t);
    }
    document
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
    /// A tenant scope for `id`; an empty id is an error.
    pub fn id(id: impl AsRef<str>) -> AppResult<Tenant> {
        let id = id.as_ref().trim();
        if id.is_empty() {
            return Err(AppError::internal("tenant id must not be empty"));
        }
        Ok(Tenant::Id(Arc::from(id)))
    }

    #[must_use]
    /// Whether the scope restricts access to one tenant.
    pub fn is_scoped(&self) -> bool {
        !matches!(self, Tenant::Global)
    }

    pub(super) fn value(&self) -> Option<Bson> {
        match self {
            Tenant::Global => None,
            Tenant::Id(id) => Some(Bson::String(id.to_string())),
            Tenant::Deny => Some(Bson::String(DENY_TENANT.to_string())),
        }
    }
}

/// Field carried by every tenant-owned MongoDB document.
pub const TENANT_FIELD: &str = "tenant_id";

/// Runs `fut` with `tenant` as the ambient tenant of every multi-tenant
/// database handle opened inside it (the request middleware wraps each
/// authenticated request this way). The value does not follow a `tokio::spawn`,
/// which is deliberate: work that escapes the request scope fails closed.
pub async fn with_tenant<F: Future>(tenant: Tenant, fut: F) -> F::Output {
    CURRENT_TENANT.scope(tenant, fut).await
}
