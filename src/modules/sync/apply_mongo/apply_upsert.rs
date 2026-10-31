use super::sync_apply_mongo::ExistingRow;
use super::sync_apply_mongo::WriteOutcome;
use super::sync_apply_mongo::build_document;
use super::sync_apply_mongo::millis;
use super::sync_apply_mongo::note_scope;
use super::sync_apply_mongo::num_i64;
use super::sync_apply_mongo::required_i64;
use super::sync_apply_mongo::required_str;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};

use crate::domain::sync_v2::{ChangeRecord, ConflictKind, DerivedScope, NewConflict};
use crate::modules::sync::cloud_store::coll;
use crate::modules::sync::mongo_codec;

use crate::modules::sync::resources::ResourceSpec;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document, doc};
use serde_json::Value;

/// The unique-valued columns of a resource. A NEW row colliding with another
/// entity on one of these is stored with a device-tagged suffix and reported
/// as a `UniqueViolation` instead of being dropped or blocking the batch.
fn unique_columns(resource: &str) -> &'static [&'static str] {
    match resource {
        "products" => &["sku"],
        "categories" | "suppliers" => &["name"],
        "invoices" => &["invoice_number"],
        "creditNotes" => &["credit_note_number"],
        "quotations" => &["quotation_number"],
        "repairs" | "printJobs" => &["ticket_number"],
        "productSerials" => &["serial_number"],
        "users" => &["username"],
        "registerSessions" => &["session_number"],
        "cashMovements" => &["movement_number"],
        "expenses" => &["expense_number"],
        "purchaseReceipts" => &["receipt_number"],
        _ => &[],
    }
}

fn device_tag(device_id: &str) -> String {
    device_id
        .trim_start_matches("dev_")
        .chars()
        .take(4)
        .collect()
}

/// Resolves a collision on a unique column deterministically: whichever key
/// sorts LOWER always keeps the value unchanged; the higher key's row is
/// always the one suffixed with a device tag. This is computed the same way
/// regardless of which of the two rows this replica happens to be inserting
/// right now and regardless of arrival order, so every replica that ever sees
/// both rows reaches the identical assignment - unlike "whichever arrives
/// second gets renamed", which depends on push order and lets two replicas
/// swap which physical row keeps the original value.
///
/// When the row that must move aside is the one already stored (the incoming
/// key is the lower one), it is corrected here rather than left alone: a real
/// document write, so the change stream (the only thing devices ever learn
/// from) carries the correction out like any other change.
async fn resolve_unique_collisions(
    db: &Db,
    spec: &ResourceSpec,
    key: &str,
    document: &mut Document,
    effective_ms: i64,
    device_id: &str,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<()> {
    let collection = coll(db, spec.table)?;
    for column in unique_columns(spec.name) {
        let Ok(value) = document.get_str(column).map(str::to_string) else {
            continue;
        };
        let Some(clash) = collection
            .find_one(doc! { *column: &value, "key": { "$ne": key }, "deleted_at": Bson::Null })
            .await?
        else {
            continue;
        };
        let Ok(clash_key) = clash.get_str("key").map(str::to_string) else {
            continue;
        };

        if key < clash_key.as_str() {
            // The incoming row is the deterministic winner: it keeps `value`
            // unchanged. The already-stored clash row moves aside instead.
            let clash_tag = device_tag(required_str(&clash, "updated_by_device")?);
            let stored = format!("{value}-{clash_tag}");
            let clash_version = required_i64(&clash, "version")?;
            collection
                .update_one(
                    doc! { "key": &clash_key },
                    doc! { "$set": {
                        *column: stored.as_str(),
                        "updated_at": millis(effective_ms),
                        "updated_by_device": device_id,
                        "version": clash_version + 1,
                    }},
                )
                .await?;
            conflicts.push(NewConflict {
                kind: ConflictKind::UniqueViolation,
                resource: spec.name.to_string(),
                entity_key: clash_key,
                detail: serde_json::json!({
                    "column": column,
                    "originalValue": value,
                    "storedValue": stored,
                }),
            });
        } else {
            let stored = format!("{value}-{}", device_tag(device_id));
            document.insert(*column, stored.as_str());
            conflicts.push(NewConflict {
                kind: ConflictKind::UniqueViolation,
                resource: spec.name.to_string(),
                entity_key: key.to_string(),
                detail: serde_json::json!({
                    "column": column,
                    "originalValue": value,
                    "storedValue": stored,
                }),
            });
        }
    }
    Ok(())
}

/// Writes an upsert change (the change already won its merge decision).
pub(crate) async fn apply_upsert(
    db: &Db,
    spec: &ResourceSpec,
    record: &ChangeRecord,
    effective_ms: i64,
    device_id: &str,
    existing: Option<&ExistingRow>,
    // Outputs of the write: conflicts raised and the derived-recompute scope it touches.
    (conflicts, scope): (&mut Vec<NewConflict>, &mut DerivedScope),
) -> AppResult<WriteOutcome> {
    let payload = record
        .payload
        .as_ref()
        .ok_or_else(|| AppError::validation("upsert without payload"))?;
    let encoded = mongo_codec::encode(spec, payload)?;

    // A serial sold on two devices: `decide` (the caller) already picked the
    // winner using the same `(updated_at_ms, device_id)` tiebreak every other
    // Lww resource uses - this only ADDS a conflict note for the owner. It must
    // never override which write actually lands: a separate "first push wins"
    // rule here would disagree with what a receiving device computes for the
    // very same two writes via its own `decide`, and the two sides would never
    // re-converge (the loser would be told `Applied` while nothing was written).
    if spec.name == "productSerials"
        && let Some(existing) = existing
        && let Some(conflict) = serial_double_sale(existing, payload, &record.key)
    {
        conflicts.push(conflict);
    }

    let mut document = build_document(spec, record, encoded, existing, effective_ms, device_id);
    let collection = coll(db, spec.table)?;

    match existing {
        Some(_) => {
            collection
                .replace_one(doc! { "key": &record.key }, document.clone())
                .await?;
        }
        None => {
            resolve_unique_collisions(
                db,
                spec,
                &record.key,
                &mut document,
                effective_ms,
                device_id,
                conflicts,
            )
            .await?;
            if let Err(err) = collection.insert_one(document.clone()).await {
                // `_id` is unique across ALL tenants of a collection. A pushed id that is
                // already taken (by another tenant, or forged) refuses just that change;
                // the message stays generic so it cannot be used to probe other tenants.
                let message = err.to_string();
                if message.contains("E11000") {
                    return Ok(WriteOutcome::Rejected(if message.contains("_id_") {
                        "ID_CONFLICT"
                    } else {
                        "UNIQUE_CONFLICT"
                    }));
                }
                return Err(err.into());
            }
        }
    }

    if spec.name == "categories" {
        fold_subcategories(db, record, payload, effective_ms, device_id).await?;
    }
    if spec.name == "users"
        && payload.get("role").and_then(Value::as_str) == Some("admin")
        && let Some(conflict) = extra_admin_conflict(db, &record.key, conflicts).await?
    {
        conflicts.push(conflict);
    }
    note_scope(spec, Some(payload), &record.key, &document, scope);
    Ok(WriteOutcome::Written)
}

/// The one-admin-per-shop rule is enforced only by the users service create
/// path, which sync bypasses. When two DIFFERENT admins arrive for one shop
/// (created on two devices), both are kept - dropping either would lock a real
/// owner out - and the shop owner is told through a conflict. There is no
/// dedicated conflict kind, so this reuses `UniqueViolation` with
/// `reason = MULTIPLE_ADMINS` (the value that is "unique" is the admin role).
async fn extra_admin_conflict(
    db: &Db,
    key: &str,
    raised: &[NewConflict],
) -> AppResult<Option<NewConflict>> {
    let mut cursor = coll(db, "users")?
        .find(doc! { "role": "admin", "deleted_at": Bson::Null })
        .await?;
    let mut admin_keys = Vec::new();
    while cursor.advance().await? {
        if let Ok(k) = cursor.deserialize_current()?.get_str("key") {
            admin_keys.push(k.to_string());
        }
    }
    admin_keys.sort();
    if admin_keys.len() < 2 || !admin_keys.iter().any(|k| k == key) {
        return Ok(None);
    }
    let same_batch = raised
        .iter()
        .any(|c| c.resource == "users" && c.entity_key == key);
    let stored = coll(db, crate::modules::sync::cloud_store::CONFLICTS)?
        .count_documents(doc! {
            "kind": ConflictKind::UniqueViolation.as_str(),
            "resource": "users", "entity_key": key, "detail.reason": "MULTIPLE_ADMINS",
        })
        .await?;
    if same_batch || stored > 0 {
        return Ok(None);
    }
    Ok(Some(NewConflict {
        kind: ConflictKind::UniqueViolation,
        resource: "users".to_string(),
        entity_key: key.to_string(),
        detail: serde_json::json!({
            "column": "role", "originalValue": "admin", "storedValue": "admin",
            "reason": "MULTIPLE_ADMINS", "adminKeys": admin_keys,
        }),
    }))
}

/// Serial double-sale rule: an existing SOLD serial receiving a different
/// invoice's sale keeps the earlier sale (returned conflict, no write).
fn serial_double_sale(existing: &ExistingRow, payload: &Value, key: &str) -> Option<NewConflict> {
    let existing_invoice = existing.doc.get_str("invoice_key").ok()?;
    let incoming_invoice = payload.get("invoiceKey").and_then(Value::as_str)?;
    let both_sold = existing.doc.get_str("status").ok() == Some("sold")
        && payload.get("status").and_then(Value::as_str) == Some("sold");
    if !both_sold || existing_invoice == incoming_invoice {
        return None;
    }
    Some(NewConflict {
        kind: ConflictKind::SerialDoubleSold,
        resource: "productSerials".to_string(),
        entity_key: key.to_string(),
        detail: serde_json::json!({
            "serialNumber": existing.doc.get_str("serial_number").ok(),
            "invoiceKeys": [existing_invoice, incoming_invoice],
        }),
    })
}

/// Upserts each subcategory carried by a category payload and tombstones the
/// ones the category no longer lists (the wire folds them into the category).
async fn fold_subcategories(
    db: &Db,
    record: &ChangeRecord,
    payload: &Value,
    effective_ms: i64,
    device_id: &str,
) -> AppResult<()> {
    let subs = coll(db, "subcategories")?;
    let listed: Vec<Value> = payload
        .get("subcategories")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut keep = Vec::new();
    for sub in &listed {
        let encoded = mongo_codec::encode_generic("subcategories", &[], sub)?;
        let key = sub
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("subcategory has no key"))?
            .to_string();
        keep.push(key.clone());

        let existing = subs.find_one(doc! { "key": &key }).await?;
        let mut document = Document::new();
        let id: ObjectId = existing
            .as_ref()
            .and_then(|d| d.get_object_id("_id").ok())
            .unwrap_or(encoded.id);
        document.insert("_id", id);
        document.insert("key", key.as_str());
        for (field, value) in encoded.fields {
            if field != "key" {
                document.insert(field, value);
            }
        }
        document.insert("category_key", record.key.as_str());
        let created = existing
            .as_ref()
            .and_then(|d| d.get_datetime("created_at").ok().copied())
            .or(encoded.created_at)
            .unwrap_or_else(|| millis(effective_ms));
        document.insert("created_at", created);
        document.insert("updated_at", millis(effective_ms));
        document.insert(
            "version",
            existing
                .as_ref()
                .and_then(|d| num_i64(d, "version"))
                .unwrap_or(0)
                .max(encoded.version),
        );
        document.insert("updated_by_device", device_id);
        if let Some(deleted_at) = encoded.deleted_at {
            document.insert("deleted_at", deleted_at);
        }
        if existing.is_some() {
            subs.replace_one(doc! { "key": &key }, document).await?;
        } else {
            subs.insert_one(document).await?;
        }
    }

    subs.update_many(
        doc! {
            "category_key": &record.key,
            "key": { "$nin": keep },
            "deleted_at": Bson::Null,
        },
        doc! { "$set": {
            "deleted_at": millis(effective_ms),
            "updated_at": millis(effective_ms),
            "updated_by_device": device_id,
        }},
    )
    .await?;
    Ok(())
}
