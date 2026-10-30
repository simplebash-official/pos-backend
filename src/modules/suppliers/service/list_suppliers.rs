use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::core::utils::build_bson_regex;
use crate::domain::suppliers::{SupplierListQuery, SuppliersResponse};
use crate::modules::suppliers::model::SupplierDocument;
use crate::modules::suppliers::repository;

use mongodb::bson::{Document, doc};

/// Builds the Mongo filter from query params (free-text search across
/// name/contactPerson/primaryPhone/address, plus a `suppliedCategories`
/// membership filter) — same construction style as
/// `inventory::service::product::list_products`. Not paginated: a shop's
/// supplier list is small enough to return in full.
pub async fn list_suppliers(db: &Db, query: SupplierListQuery) -> AppResult<SuppliersResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = build_bson_regex(&search);
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "contact_person": { "$regex": pattern.clone() } },
                { "primary_phone": { "$regex": pattern.clone() } },
                { "contact_persons.name": { "$regex": pattern.clone() } },
                { "contact_persons.phoneNumbers": { "$regex": pattern.clone() } },
                { "contact_persons.phone_numbers": { "$regex": pattern.clone() } },
                { "address": { "$regex": pattern } },
            ]
        });
    }
    if let Some(category) = query.category.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "supplied_categories": category });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let documents = repository::list_suppliers(db, filter).await?;
    let suppliers = documents
        .into_iter()
        .map(SupplierDocument::into_supplier)
        .collect();

    Ok(SuppliersResponse { suppliers })
}
