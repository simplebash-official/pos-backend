use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::quotations::model::QuotationDocument;

pub(super) fn quotations(db: &TenantDatabase) -> ScopedCollection<QuotationDocument> {
    db.collection("quotations")
}
