use super::tenant::Tenant;
use super::tenant_field::TENANT_FIELD;

use mongodb::bson::Document;

/// Adds the tenant to a document about to be inserted, overwriting any
/// `tenant_id` the caller supplied.
#[must_use]
pub fn stamp_document(tenant: &Tenant, mut document: Document) -> Document {
    if let Some(t) = tenant.value() {
        document.insert(TENANT_FIELD, t);
    }
    document
}
