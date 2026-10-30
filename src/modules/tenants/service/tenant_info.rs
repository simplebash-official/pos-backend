use crate::modules::tenants::model::TenantDocument;

/// A directory entry as returned to callers (the binary, tests).
#[derive(Debug, Clone, serde::Serialize)]
pub struct TenantInfo {
    pub key: String,
    pub shop_code: String,
    pub name: String,
}

impl From<TenantDocument> for TenantInfo {
    fn from(d: TenantDocument) -> Self {
        TenantInfo {
            key: d.key,
            shop_code: d.shop_code,
            name: d.name,
        }
    }
}
