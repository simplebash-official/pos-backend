use super::product_serials::product_serials;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductSerialDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_product_serial(
    unit: impl Into<Unit<'_>>,
    serial: ProductSerialDocument,
) -> AppResult<ProductSerialDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = product_serials(db).insert_one_in(&serial, session).await?;
            let mut inserted = serial;
            inserted.id = result.inserted_id.as_object_id();
            Ok(inserted)
        }
        Backend::Sqlite(mut conn) => {
            let id = serial
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let sold_at_iso = serial.sold_at.as_ref().map(bson_to_iso);
            let warranty_expires_at_iso = serial.warranty_expires_at.as_ref().map(bson_to_iso);
            let created_at_iso = bson_to_iso(&serial.created_at);
            let updated_at_iso = bson_to_iso(&serial.updated_at);

            sqlx::query(
                r#"
                INSERT INTO product_serials (
                    key, id, product_key, serial_number, batch_key, status, invoice_key,
                    sold_at, warranty_months, warranty_expires_at, credit_note_key,
                    version, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&serial.key)
            .bind(&id)
            .bind(&serial.product_key)
            .bind(&serial.serial_number)
            .bind(&serial.batch_key)
            .bind(serial.status.as_str())
            .bind(&serial.invoice_key)
            .bind(&sold_at_iso)
            .bind(serial.warranty_months)
            .bind(&warranty_expires_at_iso)
            .bind(&serial.credit_note_key)
            .bind(serial.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(&mut *conn)
            .await?;

            let mut inserted = serial;
            inserted.id = ObjectId::parse_str(&id).ok();
            Ok(inserted)
        }
    }
}
