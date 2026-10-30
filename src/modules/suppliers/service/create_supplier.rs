use super::resolve_and_validate_contacts::resolve_and_validate_contacts;
use super::sanitize_string::sanitize_string;
use super::validate_email::validate_email;
use super::validate_required_fields::validate_required_fields;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;

use crate::domain::suppliers::{CreateSupplierRequest, Supplier};
use crate::modules::suppliers::model::SupplierDocument;
use crate::modules::suppliers::repository;

use mongodb::bson::DateTime as BsonDateTime;

pub async fn create_supplier(db: &Db, body: CreateSupplierRequest) -> AppResult<Supplier> {
    crate::core::logging::domain::tracked("suppliers.created", async move {
        let (contact_persons, contact_person, primary_phone, secondary_phone) =
            resolve_and_validate_contacts(
                body.contact_persons,
                body.contact_person,
                body.primary_phone,
                body.secondary_phone,
            )?;

        validate_required_fields(
            &body.name,
            &contact_person,
            &primary_phone,
            &body.address,
            &body.supplied_categories,
        )?;

        let email = sanitize_string(body.email);
        if let Some(ref e) = email {
            validate_email(e)?;
        }

        let notes = sanitize_string(body.notes);

        let now = BsonDateTime::now();
        let document = SupplierDocument {
            id: None,
            key: generate_id(prefixes::SUPPLIER),
            name: body.name.trim().to_string(),
            contact_person,
            primary_phone,
            secondary_phone,
            contact_persons,
            address: body.address.trim().to_string(),
            supplied_categories: body.supplied_categories,
            email,
            notes,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        };

        let inserted = repository::insert_supplier(db, document).await?;
        Ok(inserted.into_supplier())
    })
    .await
}
