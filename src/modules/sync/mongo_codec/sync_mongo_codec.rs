use crate::core::error::{AppError, AppResult};
use crate::modules::sync::resources::ResourceSpec;
use chrono::DateTime;
use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, DateTime as BsonDateTime, Document};
use serde_json::Value;

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

/// Converts a hydrated DTO payload into the parts of a stored document.
pub(crate) fn encode(spec: &ResourceSpec, payload: &Value) -> AppResult<Encoded> {
    encode_generic(spec.name, &plain_derived(spec), payload)
}

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
    "received_at",
];

/// DTO fields that are computed on read (never stored), per wire resource name.
fn enrichment_fields(resource: &str) -> &'static [&'static str] {
    match resource {
        "products" => &[
            "category",
            "subcategory",
            "priceTiers",
            "lastSellingPriceCents",
        ],
        "purchases" => &[
            "product",
            "supplier",
            "landedTotalCents",
            "landedUnitCostCents",
        ],
        "receiptSettlements" => &["reversed"],
        "supplierCredits" => &["voidedByKey"],
        "purchaseReceipts" => &[
            "supplier",
            "lines",
            "goodsCents",
            "extraChargesCents",
            "paidToOthersCents",
            "charges",
            "totalCents",
            "creditedCents",
            "paidCents",
            "dueCents",
            "status",
            "settlements",
        ],
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

/// The stored name of a DTO field. Almost always `camelCase -> snake_case`; the
/// one explicit `#[serde(rename)]` in the DTOs is the stock movement's `type`,
/// stored as `movement_type`.
fn stored_name(resource: &str, camel: &str) -> String {
    match (resource, camel) {
        ("stockMovements", "type") => "movement_type".to_string(),
        _ => camel_to_snake(camel),
    }
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
            || (matches!(name, "purchases" | "purchaseReceipts") && snake == "date")
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
    let version = object.get("version").and_then(Value::as_i64).unwrap_or(1);

    Ok(Encoded {
        id,
        fields,
        derived,
        created_at,
        version,
        deleted_at,
    })
}

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
