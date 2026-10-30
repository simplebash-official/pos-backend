use super::tenant::Tenant;
use super::tenant_field::TENANT_FIELD;
use crate::core::error::{AppError, AppResult};
use mongodb::bson::{Bson, Document, doc};

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
