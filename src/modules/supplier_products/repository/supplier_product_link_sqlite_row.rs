use crate::clients::sqlite::to_bson_datetime;

use crate::modules::supplier_products::model::SupplierProductLinkDocument;

use mongodb::bson::oid::ObjectId;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SupplierProductLinkSqliteRow {
    pub(super) id: String,
    pub(super) key: String,
    pub(super) supplier_key: String,
    pub(super) product_key: String,
    pub(super) cost_price_cents: Option<i64>,
    pub(super) notes: Option<String>,
    pub(super) version: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) deleted_at: Option<String>,
    pub(super) updated_by_device: Option<String>,
}

impl SupplierProductLinkSqliteRow {
    pub(super) fn into_document(self) -> SupplierProductLinkDocument {
        SupplierProductLinkDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            supplier_key: self.supplier_key,
            product_key: self.product_key,
            cost_price_cents: self.cost_price_cents,
            notes: self.notes,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}
