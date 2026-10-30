use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::employees::model::EmployeeDocument;

pub(super) fn employees(db: &TenantDatabase) -> ScopedCollection<EmployeeDocument> {
    db.collection("employees")
}
