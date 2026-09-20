// Wire payload -> stored MongoDB document (the inverse of the per-module
// `hydrate_sync_documents`). The wire DTO is camelCase with RFC3339 strings and
// hex ids, plus a few read-time enrichments; the stored document is snake_case
// with typed dates/ObjectIds. Only TOP-LEVEL keys are converted: nested structs
// (`items`, `returnedItems`, `splitPayments`, ...) are the same Rust types on
// both sides and serialize identically, so they are copied verbatim.
//
// The tables below are derived from the models (`modules/*/model.rs`) and the
// DTO-vs-document field diff; `tests/sync_cloud_test.rs` round-trips real
// documents through hydrate -> encode to keep them honest.

use chrono::DateTime;
use mongodb::bson::{self, Bson, DateTime as BsonDateTime, Document, oid::ObjectId};
use serde_json::Value;

use crate::{
    core::error::{AppError, AppResult},
    modules::sync::resources::ResourceSpec,
};

/// Top-level stored fields that are BSON dates.
const DATETIME_FIELDS: &[&str] = &[
    "created_at",
    "updated_at",
    "deleted_at",
    "voided_at",
    "closed_at",
    "recorded_at",
    "sold_at",
    "warranty_expires_at",
];

/// DTO fields that are computed on read (never stored), per wire resource name.
fn enrichment_fields(resource: &str) -> &'static [&'static str] {
    match resource {
        "products" => &["category", "subcategory"],
        "purchases" => &["product", "supplier", "totalCostCents"],
        "employees" => &["login"],
        "invoices" | "repairs" | "printJobs" => &["isOverdue"],
        "supplierProducts" => &["addedAt"],
        "categories" => &["subcategories"],
        _ => &[],
    }
}

/// Columns the merge rules manage themselves, so they are handled by the
/// caller rather than copied as ordinary fields.
const WRITE_META: &[&str] = &[
    "id",
    "tenantId",
    "updatedByDevice",
    "updatedAt",
    "version",
    "createdAt",
    "deletedAt",
    "deviceId",
];

/// A payload split into what the apply step needs.
#[derive(Debug)]
pub(crate) struct Encoded {
    /// Legacy `_id`, preserved from the payload's `id`.
    pub id: ObjectId,
    /// Ordinary (non-derived) stored fields, snake_case, typed.
    pub fields: Document,
    /// Derived columns as sent (used only as the initial value of a new row).
    pub derived: Document,
    pub created_at: Option<BsonDateTime>,
    pub version: i64,
    /// `deletedAt` carried by an upsert payload (a tombstoned row).
    pub deleted_at: Option<BsonDateTime>,
}

/// The stored name of a DTO field. Almost always `camelCase -> snake_case`; the
/// one explicit `#[serde(rename)]` in the DTOs is the stock movement's `type`,
/// stored as `movement_type`.
fn stored_name(resource: &str, camel: &str) -> String {
    match (resource, camel) {
        ("stockMovements", "type") => "movement_type".to_string(),
        _ => camel_to_snake(camel),
    }
}

pub(crate) fn camel_to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Rejects keys that would be interpreted as query/update operators once
/// stored (`$...`) anywhere in the payload: a device controls this JSON.
fn reject_operator_keys(value: &Value) -> AppResult<()> {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if key.starts_with('$') {
                    return Err(AppError::validation(format!(
                        "payload key '{key}' is not allowed"
                    )));
                }
                reject_operator_keys(inner)?;
            }
            Ok(())
        }
        Value::Array(items) => items.iter().try_for_each(reject_operator_keys),
        _ => Ok(()),
    }
}

fn parse_datetime(field: &str, value: &Value) -> AppResult<BsonDateTime> {
    let text = value
        .as_str()
        .ok_or_else(|| AppError::validation(format!("'{field}' must be an RFC3339 string")))?;
    let parsed = DateTime::parse_from_rfc3339(text)
        .map_err(|_| AppError::validation(format!("'{field}' is not a valid RFC3339 date")))?;
    Ok(BsonDateTime::from_millis(parsed.timestamp_millis()))
}

pub(crate) fn parse_object_id(text: &str) -> AppResult<ObjectId> {
    ObjectId::parse_str(text).map_err(|_| AppError::validation("payload id is not a valid id"))
}

/// The top-level derived columns of a resource. The spec also lists nested
/// (`items[].returned_quantity`) and lifecycle (`status`) entries; those are
/// merged by other rules and must not be treated as plain derived columns.
pub(crate) fn plain_derived(spec: &ResourceSpec) -> Vec<&'static str> {
    spec.derived
        .iter()
        .copied()
        .filter(|c| !c.contains('[') && *c != "status")
        .collect()
}

/// Converts a hydrated DTO payload into the parts of a stored document.
pub(crate) fn encode(spec: &ResourceSpec, payload: &Value) -> AppResult<Encoded> {
    encode_generic(spec.name, &plain_derived(spec), payload)
}

/// `encode` for a resource named `name` with the given derived columns; also
/// used for sub-entities that have no `ResourceSpec` of their own
/// (subcategories, folded into their category).
pub(crate) fn encode_generic(
    name: &str,
    derived_columns: &[&str],
    payload: &Value,
) -> AppResult<Encoded> {
    let object = payload
        .as_object()
        .ok_or_else(|| AppError::validation("payload must be an object"))?;
    reject_operator_keys(payload)?;

    // Categories and subcategories carry no legacy id on the wire (they are
    // referenced by `key` only), so one is minted when it is absent.
    let id = match object.get("id").and_then(Value::as_str) {
        Some(text) => parse_object_id(text)?,
        None => ObjectId::new(),
    };
    let enrich = enrichment_fields(name);

    let mut fields = Document::new();
    let mut derived = Document::new();
    for (camel, value) in object {
        if WRITE_META.contains(&camel.as_str()) || enrich.contains(&camel.as_str()) {
            continue;
        }
        let snake = stored_name(name, camel);
        let converted = if value.is_null() {
            // Absent optional fields are simply not stored.
            continue;
        } else if DATETIME_FIELDS.contains(&snake.as_str())
            || (name == "purchases" && snake == "date")
        {
            Bson::DateTime(parse_datetime(&snake, value)?)
        } else if name == "stockMovements" && snake == "product_id" {
            Bson::ObjectId(parse_object_id(
                value
                    .as_str()
                    .ok_or_else(|| AppError::validation("productId must be a string"))?,
            )?)
        } else {
            bson::serialize_to_bson(value)
                .map_err(|e| AppError::validation(format!("'{camel}' cannot be stored: {e}")))?
        };
        if derived_columns.contains(&snake.as_str()) {
            derived.insert(snake, converted);
        } else {
            fields.insert(snake, converted);
        }
    }

    let created_at = object
        .get("createdAt")
        .filter(|v| !v.is_null())
        .map(|v| parse_datetime("createdAt", v))
        .transpose()?;
    let deleted_at = object
        .get("deletedAt")
        .filter(|v| !v.is_null())
        .map(|v| parse_datetime("deletedAt", v))
        .transpose()?;
    let version = object
        .get("version")
        .and_then(Value::as_i64)
        .unwrap_or(1);

    Ok(Encoded {
        id,
        fields,
        derived,
        created_at,
        version,
        deleted_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sync::resources::spec;
    use serde_json::json;

    #[test]
    fn snake_case_conversion() {
        assert_eq!(camel_to_snake("costPriceCents"), "cost_price_cents");
        assert_eq!(camel_to_snake("cardLast4"), "card_last4");
        assert_eq!(camel_to_snake("key"), "key");
        assert_eq!(camel_to_snake("sourceTicketKey"), "source_ticket_key");
    }

    #[test]
    fn product_payload_is_split_into_fields_and_derived() {
        let product = spec("products").unwrap();
        let encoded = encode(
            product,
            &json!({
                "id": "65a1b2c3d4e5f60718293a4b", "key": "prd_1", "name": "Screen",
                "category": "Phones", "subcategory": "Screens", "categoryKey": "cat_1",
                "stockQuantity": 7, "sellingPriceCents": 1000, "version": 3,
                "createdAt": "2026-05-01T10:00:00.000Z", "updatedAt": "2026-05-01T11:00:00.000Z",
                "deletedAt": null, "updatedByDevice": "dev_x"
            }),
        )
        .unwrap();
        assert_eq!(encoded.id.to_hex(), "65a1b2c3d4e5f60718293a4b");
        assert_eq!(encoded.fields.get_str("name").unwrap(), "Screen");
        assert_eq!(encoded.fields.get_str("category_key").unwrap(), "cat_1");
        // Read-time enrichments and merge-managed columns are not stored fields.
        for absent in ["category", "subcategory", "updated_at", "version", "id", "updated_by_device"] {
            assert!(encoded.fields.get(absent).is_none(), "{absent}");
        }
        assert_eq!(encoded.derived.get_i64("stock_quantity").unwrap(), 7);
        assert_eq!(encoded.version, 3);
        assert!(encoded.created_at.is_some());
        assert!(encoded.deleted_at.is_none());
    }

    #[test]
    fn stock_movement_product_id_becomes_an_object_id() {
        let movement = spec("stockMovements").unwrap();
        let encoded = encode(
            movement,
            &json!({ "id": "65a1b2c3d4e5f60718293a4b", "key": "sm_1",
                     "productId": "65a1b2c3d4e5f60718293a4c", "quantityDelta": -2 }),
        )
        .unwrap();
        assert!(matches!(encoded.fields.get("product_id"), Some(Bson::ObjectId(_))));
    }

    #[test]
    fn payloads_with_operator_keys_or_bad_ids_are_rejected() {
        let product = spec("products").unwrap();
        let ok_id = "65a1b2c3d4e5f60718293a4b";
        assert!(encode(product, &json!({ "id": ok_id, "meta": { "$set": 1 } })).is_err());
        assert!(encode(product, &json!({ "id": "nope" })).is_err());
        assert!(encode(product, &json!({ "id": ok_id, "createdAt": "yesterday" })).is_err());
        assert!(encode(product, &json!([1, 2])).is_err());
    }

    #[test]
    fn payload_without_id_gets_a_fresh_one_and_lifecycle_status_is_not_derived() {
        let category = spec("categories").unwrap();
        let encoded = encode(category, &json!({ "key": "cat_1", "name": "Phones" })).unwrap();
        assert_eq!(encoded.fields.get_str("key").unwrap(), "cat_1");

        let invoice = spec("invoices").unwrap();
        let derived = plain_derived(invoice);
        assert!(derived.contains(&"refunded_cents"));
        assert!(!derived.contains(&"status"));
        assert!(derived.iter().all(|c| !c.contains('[')));
    }

    #[test]
    fn null_fields_are_omitted_not_stored() {
        let customer = spec("customers").unwrap();
        let encoded = encode(
            customer,
            &json!({ "id": "65a1b2c3d4e5f60718293a4b", "key": "cus_1", "email": null }),
        )
        .unwrap();
        assert!(encoded.fields.get("email").is_none());
    }
}
