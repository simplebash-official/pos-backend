use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::{AppError, AppResult};
use crate::domain::warranties::ClaimStatus;
use crate::modules::warranties::model::WarrantyClaimDocument;
use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document, doc};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

const CLAIMS: &str = "warranty_claims";

const CLAIM_COLUMNS: &str = "key, id, claim_number, warranty_key, warranty_number, item_name, \
     serial_number, product_key, invoice_key, invoice_number, customer_key, customer_name, \
     customer_phone, issue_description, void_checks, assigned_employee_key, \
     assigned_employee_name, status, inspection_notes, decision, decision_reason, \
     decided_by_user_id, decided_by_name, expired_override_by_name, resolution_note, \
     replacement_serial_number, replacement_warranty_key, credit_note_key, paid_repair_key, \
     paid_repair_number, rework_repair_key, rework_repair_number, supplier, received_on, resolved_on, returned_on, version, created_at, \
     updated_at, updated_by_device";

fn claims(db: &TenantDatabase) -> ScopedCollection<WarrantyClaimDocument> {
    db.collection(CLAIMS)
}

/// A serde value's stored text: an enum's name, or JSON for an object.
fn stored_text<T: serde::Serialize>(value: &T) -> AppResult<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => Ok(text),
        Ok(other @ serde_json::Value::Object(_)) => Ok(other.to_string()),
        other => Err(AppError::internal(format!(
            "claim value did not serialize to text: {other:?}"
        ))),
    }
}

fn parse_enum<T: serde::de::DeserializeOwned>(field: &str, text: &str) -> AppResult<T> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
        .map_err(|_| AppError::internal(format!("stored claim {field} `{text}` is unknown")))
}

fn parse_json<T: serde::de::DeserializeOwned>(field: &str, text: &str) -> AppResult<T> {
    serde_json::from_str(text)
        .map_err(|err| AppError::internal(format!("stored claim {field} is not readable: {err}")))
}

fn row_to_claim(row: &SqliteRow) -> AppResult<WarrantyClaimDocument> {
    let id: String = row.get("id");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    let decision: Option<String> = row.get("decision");
    let supplier: Option<String> = row.get("supplier");
    Ok(WarrantyClaimDocument {
        id: Some(
            ObjectId::parse_str(&id)
                .map_err(|_| AppError::internal(format!("stored claim id `{id}` is invalid")))?,
        ),
        key: row.get("key"),
        claim_number: row.get("claim_number"),
        warranty_key: row.get("warranty_key"),
        warranty_number: row.get("warranty_number"),
        item_name: row.get("item_name"),
        serial_number: row.get("serial_number"),
        product_key: row.get("product_key"),
        invoice_key: row.get("invoice_key"),
        invoice_number: row.get("invoice_number"),
        customer_key: row.get("customer_key"),
        customer_name: row.get("customer_name"),
        customer_phone: row.get("customer_phone"),
        issue_description: row.get("issue_description"),
        void_checks: parse_json("void_checks", row.get("void_checks"))?,
        assigned_employee_key: row.get("assigned_employee_key"),
        assigned_employee_name: row.get("assigned_employee_name"),
        status: parse_enum("status", row.get("status"))?,
        inspection_notes: row.get("inspection_notes"),
        decision: decision
            .as_deref()
            .map(|text| parse_enum("decision", text))
            .transpose()?,
        decision_reason: row.get("decision_reason"),
        decided_by_user_id: row.get("decided_by_user_id"),
        decided_by_name: row.get("decided_by_name"),
        expired_override_by_name: row.get("expired_override_by_name"),
        resolution_note: row.get("resolution_note"),
        replacement_serial_number: row.get("replacement_serial_number"),
        replacement_warranty_key: row.get("replacement_warranty_key"),
        credit_note_key: row.get("credit_note_key"),
        paid_repair_key: row.get("paid_repair_key"),
        paid_repair_number: row.get("paid_repair_number"),
        rework_repair_key: row.get("rework_repair_key"),
        rework_repair_number: row.get("rework_repair_number"),
        supplier: supplier
            .as_deref()
            .map(|text| parse_json("supplier", text))
            .transpose()?,
        received_on: row.get("received_on"),
        resolved_on: row.get("resolved_on"),
        returned_on: row.get("returned_on"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at)?,
        updated_at: to_bson_datetime(&updated_at)?,
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Binds every claim column, in `CLAIM_COLUMNS` order after `key, id`.
fn bind_fields<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    claim: &'q WarrantyClaimDocument,
) -> AppResult<sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>> {
    Ok(query
        .bind(&claim.claim_number)
        .bind(&claim.warranty_key)
        .bind(&claim.warranty_number)
        .bind(&claim.item_name)
        .bind(&claim.serial_number)
        .bind(&claim.product_key)
        .bind(&claim.invoice_key)
        .bind(&claim.invoice_number)
        .bind(&claim.customer_key)
        .bind(&claim.customer_name)
        .bind(&claim.customer_phone)
        .bind(&claim.issue_description)
        .bind(stored_text(&claim.void_checks)?)
        .bind(&claim.assigned_employee_key)
        .bind(&claim.assigned_employee_name)
        .bind(stored_text(&claim.status)?)
        .bind(&claim.inspection_notes)
        .bind(claim.decision.as_ref().map(stored_text).transpose()?)
        .bind(&claim.decision_reason)
        .bind(&claim.decided_by_user_id)
        .bind(&claim.decided_by_name)
        .bind(&claim.expired_override_by_name)
        .bind(&claim.resolution_note)
        .bind(&claim.replacement_serial_number)
        .bind(&claim.replacement_warranty_key)
        .bind(&claim.credit_note_key)
        .bind(&claim.paid_repair_key)
        .bind(&claim.paid_repair_number)
        .bind(&claim.rework_repair_key)
        .bind(&claim.rework_repair_number)
        .bind(claim.supplier.as_ref().map(stored_text).transpose()?)
        .bind(&claim.received_on)
        .bind(&claim.resolved_on)
        .bind(&claim.returned_on)
        .bind(claim.version)
        .bind(bson_to_iso(&claim.created_at))
        .bind(bson_to_iso(&claim.updated_at))
        .bind(&claim.updated_by_device))
}

/// Stores a new claim. Returns it with its id.
pub(crate) async fn insert_claim(
    db: &Db,
    mut claim: WarrantyClaimDocument,
) -> AppResult<WarrantyClaimDocument> {
    match db {
        Db::Mongo(mongo) => {
            let result = claims(mongo).insert_one(&claim).await?;
            claim.id = result.inserted_id.as_object_id();
            Ok(claim)
        }
        Db::Sqlite(pool) => {
            let id = claim.id.map_or_else(generate_id_hex, |oid| oid.to_hex());
            let placeholders = vec!["?"; CLAIM_COLUMNS.split(',').count()].join(", ");
            let sql =
                format!("INSERT INTO warranty_claims ({CLAIM_COLUMNS}) VALUES ({placeholders})");
            let query = sqlx::query(&sql).bind(&claim.key).bind(&id);
            bind_fields(query, &claim)?.execute(pool).await?;
            claim.id =
                Some(ObjectId::parse_str(&id).expect("generate_id_hex returns an ObjectId hex"));
            Ok(claim)
        }
    }
}

/// Writes a changed claim over the stored one, only if nobody changed it since it was read
/// (`version` must still be `expected_version`). `false` when it had changed.
pub(crate) async fn replace_claim(
    db: &Db,
    claim: &WarrantyClaimDocument,
    expected_version: i64,
) -> AppResult<bool> {
    match db {
        Db::Mongo(mongo) => Ok(claims(mongo)
            .replace_one(
                doc! { "key": &claim.key, "version": expected_version },
                claim,
            )
            .await?
            .matched_count
            == 1),
        Db::Sqlite(pool) => {
            let assignments = CLAIM_COLUMNS
                .split(',')
                .map(str::trim)
                .skip(2)
                .map(|column| format!("{column} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql =
                format!("UPDATE warranty_claims SET {assignments} WHERE key = ? AND version = ?");
            let result = bind_fields(sqlx::query(&sql), claim)?
                .bind(&claim.key)
                .bind(expected_version)
                .execute(pool)
                .await?;
            Ok(result.rows_affected() == 1)
        }
    }
}

/// One claim by its `wcl_` key.
pub(crate) async fn find_claim(db: &Db, key: &str) -> AppResult<Option<WarrantyClaimDocument>> {
    match db {
        Db::Mongo(mongo) => Ok(claims(mongo).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => sqlx::query(&format!(
            "SELECT {CLAIM_COLUMNS} FROM warranty_claims WHERE key = ?"
        ))
        .bind(key)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_claim)
        .transpose(),
    }
}

/// Which claims a list wants. Every set field narrows it.
#[derive(Debug, Default)]
pub(crate) struct ClaimFilter<'a> {
    /// Only these statuses; `None` for every status.
    pub statuses: Option<&'a [ClaimStatus]>,
    pub warranty_key: Option<&'a str>,
    pub invoice_key: Option<&'a str>,
    pub supplier_key: Option<&'a str>,
    pub assigned_employee_key: Option<&'a str>,
    /// Part of the claim or warranty number, serial, item, customer or phone (any case).
    pub text: Option<&'a str>,
}

/// Columns a text search looks in.
const SEARCHED: [&str; 6] = [
    "claim_number",
    "warranty_number",
    "serial_number",
    "item_name",
    "customer_name",
    "customer_phone",
];

impl ClaimFilter<'_> {
    fn mongo(&self) -> Document {
        let mut filter = Document::new();
        if let Some(statuses) = self.statuses {
            let names = statuses
                .iter()
                .map(|status| Bson::String(status.as_str().to_string()))
                .collect::<Vec<_>>();
            filter.insert("status", doc! { "$in": names });
        }
        for (field, value) in [
            ("warranty_key", self.warranty_key),
            ("invoice_key", self.invoice_key),
            ("supplier.supplierKey", self.supplier_key),
            ("assigned_employee_key", self.assigned_employee_key),
        ] {
            if let Some(value) = value {
                filter.insert(field, value);
            }
        }
        if let Some(text) = self.text {
            let pattern = crate::core::utils::regex_escape(text);
            let any = SEARCHED
                .iter()
                .map(|field| doc! { *field: { "$regex": &pattern, "$options": "i" } })
                .collect::<Vec<_>>();
            filter.insert("$or", any);
        }
        filter
    }

    fn sqlite(&self) -> (String, Vec<String>) {
        let mut clauses = vec!["1 = 1".to_string()];
        let mut values = Vec::new();
        if let Some(statuses) = self.statuses {
            clauses.push(format!(
                "status IN ({})",
                vec!["?"; statuses.len()].join(", ")
            ));
            values.extend(statuses.iter().map(|s| s.as_str().to_string()));
        }
        for (column, value) in [
            ("warranty_key", self.warranty_key),
            ("invoice_key", self.invoice_key),
            ("json_extract(supplier, '$.supplierKey')", self.supplier_key),
            ("assigned_employee_key", self.assigned_employee_key),
        ] {
            if let Some(value) = value {
                clauses.push(format!("{column} = ?"));
                values.push(value.to_string());
            }
        }
        if let Some(text) = self.text {
            // `instr` keeps `%` and `_` literal, unlike LIKE.
            let any = SEARCHED
                .iter()
                .map(|column| format!("instr(lower(coalesce({column}, '')), lower(?)) > 0"))
                .collect::<Vec<_>>()
                .join(" OR ");
            clauses.push(format!("({any})"));
            values.extend(SEARCHED.iter().map(|_| text.to_string()));
        }
        (clauses.join(" AND "), values)
    }
}

/// A page of claims matching `filter`, oldest first (what has waited longest on top), and how
/// many match in all.
pub(crate) async fn list_claims(
    db: &Db,
    filter: &ClaimFilter<'_>,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<WarrantyClaimDocument>, u64)> {
    match db {
        Db::Mongo(mongo) => {
            let collection = claims(mongo);
            let query = filter.mongo();
            let total = collection.count_documents(query.clone()).await?;
            let cursor = collection
                .find(query)
                .sort(doc! { "created_at": 1, "key": 1 })
                .skip(skip)
                .limit(i64::try_from(limit).expect("page limits are small"))
                .await?;
            Ok((cursor.try_collect().await?, total))
        }
        Db::Sqlite(pool) => {
            let (clause, values) = filter.sqlite();
            let count_sql = format!("SELECT COUNT(*) FROM warranty_claims WHERE {clause}");
            let mut count = sqlx::query_scalar::<_, i64>(&count_sql);
            for value in &values {
                count = count.bind(value);
            }
            let total =
                u64::try_from(count.fetch_one(pool).await?).expect("a row count is never negative");
            let select = format!(
                "SELECT {CLAIM_COLUMNS} FROM warranty_claims WHERE {clause} \
                 ORDER BY created_at ASC, key ASC LIMIT ? OFFSET ?"
            );
            let mut query = sqlx::query(&select);
            for value in &values {
                query = query.bind(value);
            }
            let rows = query
                .bind(i64::try_from(limit).expect("page limits are small"))
                .bind(i64::try_from(skip).expect("page offsets are small"))
                .fetch_all(pool)
                .await?;
            Ok((
                rows.iter().map(row_to_claim).collect::<AppResult<_>>()?,
                total,
            ))
        }
    }
}
