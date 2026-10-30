use crate::clients::sqlite::to_bson_datetime;

use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::oid::ObjectId;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct CustomerSqliteRow {
    pub(super) id: String,
    pub(super) key: String,
    pub(super) name: String,
    pub(super) contact_person: Option<String>,
    pub(super) primary_phone: String,
    pub(super) secondary_phone: Option<String>,
    pub(super) email: Option<String>,
    pub(super) address: Option<String>,
    pub(super) tags: String,
    pub(super) notes: Option<String>,
    pub(super) outstanding_balance_cents: i64,
    pub(super) total_purchases_cents: i64,
    pub(super) version: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) deleted_at: Option<String>,
    pub(super) updated_by_device: Option<String>,
}

impl CustomerSqliteRow {
    pub(super) fn into_document(self) -> CustomerDocument {
        let tags: Vec<String> = serde_json::from_str(&self.tags).unwrap_or_default();
        CustomerDocument {
            id: ObjectId::parse_str(&self.id).ok(),
            key: self.key,
            name: self.name,
            contact_person: self.contact_person,
            primary_phone: self.primary_phone,
            secondary_phone: self.secondary_phone,
            email: self.email,
            address: self.address,
            tags,
            notes: self.notes,
            outstanding_balance_cents: self.outstanding_balance_cents,
            total_purchases_cents: self.total_purchases_cents,
            version: self.version,
            created_at: to_bson_datetime(&self.created_at),
            updated_at: to_bson_datetime(&self.updated_at),
            deleted_at: self.deleted_at.as_deref().map(to_bson_datetime),
            updated_by_device: self.updated_by_device,
        }
    }
}
