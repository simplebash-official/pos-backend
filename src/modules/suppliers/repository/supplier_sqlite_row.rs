use crate::clients::sqlite::to_bson_datetime;

use crate::domain::suppliers::ContactPerson;
use crate::modules::suppliers::model::SupplierDocument;

use mongodb::bson::oid::ObjectId;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SupplierSqliteRow {
    pub(super) id: String,
    pub(super) key: String,
    pub(super) name: String,
    pub(super) contact_person: String,
    pub(super) primary_phone: String,
    pub(super) secondary_phone: Option<String>,
    #[sqlx(default)]
    pub(super) contact_persons: Option<String>,
    pub(super) email: Option<String>,
    pub(super) address: String,
    pub(super) supplied_categories: String,
    pub(super) notes: Option<String>,
    pub(super) version: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) deleted_at: Option<String>,
    pub(super) updated_by_device: Option<String>,
}

impl SupplierSqliteRow {
    pub(super) fn into_document(self) -> SupplierDocument {
        let supplied_categories: Vec<String> =
            serde_json::from_str(&self.supplied_categories).unwrap_or_default();
        let contact_persons: Vec<ContactPerson> = self
            .contact_persons
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        SupplierDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            name: self.name,
            contact_person: self.contact_person,
            primary_phone: self.primary_phone,
            secondary_phone: self.secondary_phone,
            contact_persons,
            email: self.email,
            address: self.address,
            supplied_categories,
            notes: self.notes,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}
