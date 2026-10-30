use super::stock_movements::stock_movements;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::inventory::model::StockMovementDocument;

pub(crate) async fn insert_stock_movement(
    unit: impl Into<Unit<'_>>,
    movement: StockMovementDocument,
) -> AppResult<()> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            stock_movements(db)
                .insert_one_in(&movement, session)
                .await?;
            Ok(())
        }
        Backend::Sqlite(mut conn) => {
            let id = movement
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at_iso = bson_to_iso(&movement.created_at);
            let updated_at_iso = bson_to_iso(&movement.updated_at);
            let deleted_at_iso = movement.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO stock_movements (
                    key, id, product_id, batch_key, quantity_delta, movement_type,
                    reference_id, note, version, created_at, updated_at, deleted_at,
                    updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&movement.key)
            .bind(&id)
            .bind(movement.product_id.to_hex())
            .bind(&movement.batch_key)
            .bind(movement.quantity_delta)
            .bind(movement.movement_type.as_str())
            .bind(&movement.reference_id)
            .bind(&movement.note)
            .bind(movement.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&movement.updated_by_device)
            .execute(&mut *conn)
            .await?;

            Ok(())
        }
    }
}
