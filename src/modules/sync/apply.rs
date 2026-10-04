// Device-side applier (SQLite): writes a batch of canonical change records into
// the local database inside ONE `BEGIN IMMEDIATE` transaction.
//
//   1. `sync_state.applying = 1` (so the capture triggers stay silent - remote
//      changes must never echo into the outbox), cleared again before COMMIT;
//   2. per change (parents before children): `sync_merge::decide` against the
//      row's `sync_row_meta`, then a generic column-driven upsert / tombstone;
//   3. `derived_sqlite::recompute` for every entity the batch touched;
//   4. the cursor advance, all in the same transaction.
//
// Any error rolls the whole batch back (flag and cursor included) and the
// caller retries the same page, which is safe because applying is idempotent.
//
// The payload is the hydrated REST DTO (camelCase) plus the legacy `id`. It is
// mapped onto the table's own columns (`pragma_table_info`), so DTO-only
// enrichment (category names, `isOverdue`, supplier summaries...) is ignored
// and derived columns are never taken from a payload.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, query::Query, sqlite::SqliteArguments};

use crate::{
    clients::{db::Db, sqlite::generate_id_hex},
    core::{
        error::AppResult,
        sync_merge::{
            Decision, IncomingChange, RowMeta, append_only_differs, clamp_updated_at, decide,
            is_meaningful_conflict, merge_lifecycle,
        },
    },
    domain::sync_v2::{
        ApplyMode, ApplyOrigin, ApplyOutcome, ApplyRequest, ApplyResponse, ChangeOp, ChangeRecord,
        ConflictKind, DerivedScope, NewConflict,
    },
    modules::sync::{
        derived_sqlite,
        outbox::ms_to_datetime,
        resources::{
            self, MergeClass, Phase, ResourceSpec, SYNC_RESOURCES, snake_to_camel, without_secrets,
        },
        service::hydrate,
        state::pool_of,
    },
};

/// Tables the local code removes with a real `DELETE` (not a tombstone): a
/// receiver must do the same, or the row would linger and keep showing up in
/// reads that do not filter `deleted_at`. The row's merge metadata survives, so
/// an older edit arriving later still loses to the delete.
const HARD_DELETE_TABLES: &[&str] = &["categories", "supplier_products", "users"];

/// One column of a synced table.
#[derive(Debug, Clone)]
struct Col {
    name: String,
    kind: ColKind,
    not_null: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColKind {
    Integer,
    Real,
    Json,
    Text,
}

/// A value bound into a dynamically built statement.
#[derive(Debug, Clone)]
enum V {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
}

fn bind_all<'q>(
    mut query: Query<'q, Sqlite, SqliteArguments<'q>>,
    values: &'q [V],
) -> Query<'q, Sqlite, SqliteArguments<'q>> {
    for value in values {
        query = match value {
            V::Null => query.bind(None::<String>),
            V::Int(i) => query.bind(*i),
            V::Real(r) => query.bind(*r),
            V::Text(t) => query.bind(t.as_str()),
        };
    }
    query
}

async fn table_columns(conn: &mut SqliteConnection, table: &str) -> AppResult<Vec<Col>> {
    let rows = sqlx::query(r#"SELECT name, type, "notnull" AS required FROM pragma_table_info(?)"#)
        .bind(table)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let declared = row.get::<String, _>("type").to_uppercase();
            let kind = if declared.contains("JSON") {
                ColKind::Json
            } else if declared.contains("INT") {
                ColKind::Integer
            } else if declared.contains("REAL")
                || declared.contains("FLOA")
                || declared.contains("DOUB")
            {
                ColKind::Real
            } else {
                ColKind::Text
            };
            Col {
                name: row.get("name"),
                kind,
                not_null: row.get::<i64, _>("required") != 0,
            }
        })
        .collect())
}

fn is_time_column(name: &str) -> bool {
    name.ends_with("_at") || name.ends_with("_date") || name == "date"
}

/// Converts one payload value for `col`, or `None` when it cannot be stored.
fn to_sql(col: &Col, value: &Value) -> V {
    match value {
        Value::Null => V::Null,
        Value::Bool(b) => V::Int(*b as i64),
        Value::Number(n) => match col.kind {
            ColKind::Real => V::Real(n.as_f64().unwrap_or(0.0)),
            _ => match n.as_i64() {
                Some(i) => V::Int(i),
                None => V::Real(n.as_f64().unwrap_or(0.0)),
            },
        },
        Value::String(s) => {
            if is_time_column(&col.name)
                && let Ok(dt) = DateTime::parse_from_rfc3339(s)
            {
                // Same text shape every local write produces.
                V::Text(dt.with_timezone(&Utc).to_rfc3339())
            } else {
                V::Text(s.clone())
            }
        }
        Value::Array(_) | Value::Object(_) => V::Text(value.to_string()),
    }
}

/// What a row write needs to know.
#[derive(Clone, Copy)]
struct RowWrite<'a> {
    table: &'a str,
    key: &'a str,
    payload: &'a Map<String, Value>,
    ms: i64,
    version: i64,
    device: &'a str,
    derived: &'a [&'a str],
}

struct UniqueFix {
    /// Row the conflict is reported against - the one that actually got the
    /// device-tagged suffix, which is not always `w.key` (see
    /// `upsert_with_unique_retry`).
    entity_key: String,
    column: String,
    original: String,
    stored: String,
}

/// Upserts one row from a payload, mapping DTO fields to columns.
/// The wire (DTO) field carrying a stored column: plain camelCase, except the
/// one DTO field that is renamed (`type` for `stock_movements.movement_type`).
fn wire_field(table: &str, column: &str) -> String {
    match (table, column) {
        ("stock_movements", "movement_type") => "type".to_string(),
        _ => snake_to_camel(column),
    }
}

async fn upsert_row(
    conn: &mut SqliteConnection,
    cols: &[Col],
    w: &RowWrite<'_>,
) -> Result<(), sqlx::Error> {
    let mut names: Vec<&str> = Vec::new();
    let mut values: Vec<V> = Vec::new();
    let now = Utc::now().to_rfc3339();

    for col in cols {
        let name = col.name.as_str();
        let camel = wire_field(w.table, name);
        let value = match name {
            "key" => Some(V::Text(w.key.to_string())),
            "id" => Some(match w.payload.get("id").and_then(Value::as_str) {
                Some(id) if !id.is_empty() => V::Text(id.to_string()),
                _ => V::Text(generate_id_hex()),
            }),
            "version" => Some(V::Int(w.version)),
            "updated_at" => Some(V::Text(ms_to_datetime(w.ms).to_rfc3339())),
            "deleted_at" => Some(V::Null),
            "updated_by_device" => Some(V::Text(w.device.to_string())),
            "created_at" => Some(match w.payload.get("createdAt") {
                Some(v) if !v.is_null() => to_sql(col, v),
                _ => V::Text(now.clone()),
            }),
            _ => match w.payload.get(&camel) {
                Some(v) => Some(to_sql(col, v)),
                // Absent optional field = cleared; required fields keep their value.
                None if !col.not_null => Some(V::Null),
                None => None,
            },
        };
        if let Some(v) = value {
            names.push(name);
            values.push(v);
        }
    }

    let placeholders = vec!["?"; names.len()].join(", ");
    let updates: Vec<String> = names
        .iter()
        // Derived columns only seed a new row; recompute owns them afterwards.
        .filter(|n| {
            !matches!(**n, "key" | "id" | "created_at" | "version") && !w.derived.contains(n)
        })
        .map(|n| format!("{n} = excluded.{n}"))
        .chain(
            names
                .contains(&"version")
                .then(|| format!("version = MAX({t}.version, excluded.version)", t = w.table)),
        )
        .collect();
    let sql = format!(
        "INSERT INTO {table} ({cols}) VALUES ({placeholders}) ON CONFLICT(key) DO UPDATE SET {updates}",
        table = w.table,
        cols = names.join(", "),
        updates = updates.join(", "),
    );
    bind_all(sqlx::query(&sql), &values)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The column named in `UNIQUE constraint failed: table.column`.
fn violated_column(err: &sqlx::Error) -> Option<String> {
    let sqlx::Error::Database(db_err) = err else {
        return None;
    };
    if !db_err.is_unique_violation() {
        return None;
    }
    let msg = db_err.message();
    let part = msg.split("failed:").nth(1)?.split(',').next()?.trim();
    Some(part.rsplit('.').next()?.to_string())
}

/// Upserts a row; on a UNIQUE clash with a different row the incoming value is
/// stored with a device suffix instead of being dropped, and reported.
/// Upserts a row; on a UNIQUE clash with a different row, whichever key sorts
/// LOWER always keeps the value unchanged and the higher key is always the
/// one suffixed with a device tag - the same deterministic rule the Mongo
/// side uses (`apply_mongo::resolve_unique_collisions`), so every replica
/// that ever sees both rows reaches the identical assignment regardless of
/// which one it happens to be applying right now or in what order arrival
/// happens locally.
///
/// When the row that must move aside is the one ALREADY stored (the incoming
/// key is the lower one), it is corrected here in place - and, because that
/// correction is this device's own decision made while applying someone
/// else's change (inside the caller's `applying = 1` window, which normally
/// silences the capture triggers so a remote change is never echoed back),
/// it is manually re-queued into this device's own outbox exactly as a
/// trigger would, so the correction still leaves on the next push instead of
/// being silently swallowed.
async fn upsert_with_unique_retry(
    conn: &mut SqliteConnection,
    cols: &[Col],
    w: &RowWrite<'_>,
    resource: &str,
    own_tag: &str,
) -> AppResult<Option<UniqueFix>> {
    // Statement-level failure leaves the transaction usable in SQLite.
    match upsert_row(conn, cols, w).await {
        Ok(()) => Ok(None),
        Err(err) => {
            let Some(column) = violated_column(&err) else {
                return Err(err.into());
            };
            let camel = wire_field(w.table, &column);
            let Some(value) = w
                .payload
                .get(&camel)
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                return Err(err.into());
            };

            let clash: Option<(String, Option<String>)> = sqlx::query_as(&format!(
                "SELECT key, updated_by_device FROM {} WHERE {} = ? AND key != ?",
                w.table, column
            ))
            .bind(&value)
            .bind(w.key)
            .fetch_optional(&mut *conn)
            .await?;
            let Some((clash_key, clash_device)) = clash else {
                // The constraint fired for a reason this rule doesn't cover
                // (e.g. a race with a write outside sync); surface it as before.
                return Err(err.into());
            };

            if w.key < clash_key.as_str() {
                // The incoming row is the deterministic winner: it keeps `value`
                // unchanged, and the already-stored clash row moves aside.
                let clash_tag = device_tag(clash_device.as_deref().unwrap_or(own_tag));
                let stored = format!("{value}-{clash_tag}");
                rename_stored_row(conn, w.table, &clash_key, &column, &stored, resource).await?;
                upsert_row(conn, cols, w).await?; // now succeeds - the clash is cleared
                Ok(Some(UniqueFix {
                    entity_key: clash_key,
                    column,
                    original: value,
                    stored,
                }))
            } else {
                let stored = format!("{value}-{own_tag}");
                let mut patched = w.payload.clone();
                patched.insert(camel, Value::String(stored.clone()));
                let retry = RowWrite {
                    payload: &patched,
                    ..*w
                };
                upsert_row(conn, cols, &retry).await?;
                Ok(Some(UniqueFix {
                    entity_key: w.key.to_string(),
                    column,
                    original: value,
                    stored,
                }))
            }
        }
    }
}

/// Corrects an already-stored row's colliding column to `new_value` and
/// manually re-queues it into the local outbox (see `upsert_with_unique_retry`
/// for why: this write happens inside an `applying = 1` transaction, so the
/// normal capture trigger for `table` never fires for it).
async fn rename_stored_row(
    conn: &mut SqliteConnection,
    table: &str,
    key: &str,
    column: &str,
    new_value: &str,
    resource: &str,
) -> AppResult<()> {
    let own_device: String = sqlx::query_scalar("SELECT device_id FROM sync_state WHERE id = 1")
        .fetch_one(&mut *conn)
        .await?;
    let ms = local_correction_ms(conn, resource, key).await?;
    let version: i64 = sqlx::query_scalar(&format!("SELECT version FROM {table} WHERE key = ?"))
        .bind(key)
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query(&format!(
        "UPDATE {table} SET {column} = ?, updated_at = ?, updated_by_device = ?, version = ? WHERE key = ?"
    ))
    .bind(new_value)
    .bind(ms_to_datetime(ms).to_rfc3339())
    .bind(&own_device)
    .bind(version + 1)
    .bind(key)
    .execute(&mut *conn)
    .await?;
    save_meta(conn, resource, key, ms, &own_device, version + 1).await?;
    sqlx::query(
        "INSERT OR REPLACE INTO sync_outbox (resource, key, op, enqueued_at) \
         VALUES (?, ?, 'upsert', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
    )
    .bind(resource)
    .bind(key)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The `updated_at_ms` a locally-decided correction gets: this device's own
/// clock (adjusted the same way the capture triggers compute it) or one past
/// the row's current meta, whichever is later - so the correction always wins
/// a future Lww comparison against the state it is replacing.
async fn local_correction_ms(
    conn: &mut SqliteConnection,
    resource: &str,
    key: &str,
) -> AppResult<i64> {
    let offset: i64 = sqlx::query_scalar("SELECT clock_offset_ms FROM sync_state WHERE id = 1")
        .fetch_one(&mut *conn)
        .await?;
    let prior: Option<i64> = sqlx::query_scalar(
        "SELECT updated_at_ms FROM sync_row_meta WHERE resource = ? AND key = ?",
    )
    .bind(resource)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    let now = Utc::now().timestamp_millis() + offset;
    Ok(now.max(prior.unwrap_or(0) + 1))
}

fn device_tag(device_id: &str) -> String {
    device_id
        .strip_prefix("dev_")
        .unwrap_or(device_id)
        .chars()
        .take(4)
        .collect()
}

async fn load_meta(
    conn: &mut SqliteConnection,
    resource: &str,
    key: &str,
) -> AppResult<Option<RowMeta>> {
    let row = sqlx::query(
        "SELECT updated_at_ms, device_id, version FROM sync_row_meta WHERE resource = ? AND key = ?",
    )
    .bind(resource)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| RowMeta {
        updated_at_ms: r.get("updated_at_ms"),
        device_id: r.get("device_id"),
        version: r.get("version"),
        deleted: false,
    }))
}

async fn save_meta(
    conn: &mut SqliteConnection,
    resource: &str,
    key: &str,
    ms: i64,
    device: &str,
    version: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO sync_row_meta (resource, key, updated_at_ms, device_id, version) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(resource)
    .bind(key)
    .bind(ms)
    .bind(device)
    .bind(version)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The local row as the DTO payload sync would send for it.
async fn local_payload(db: &Db, spec: &ResourceSpec, key: &str) -> AppResult<Option<Value>> {
    let pool = pool_of(db)?;
    let Some(row) = sqlx::query(&format!("SELECT * FROM {} WHERE key = ?", spec.table))
        .bind(key)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    let document = crate::clients::sqlite::map_sqlite_row_to_document(&row);
    Ok(hydrate(db, spec.name, vec![document]).await?.pop())
}

fn add_scope(spec: &ResourceSpec, rec: &ChangeRecord, scope: &mut DerivedScope) {
    let text = |field: &str| {
        rec.payload
            .as_ref()
            .and_then(|p| p.get(field))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    match spec.name {
        "products" => scope.product_ids.extend(text("id")),
        "stockMovements" => scope.product_ids.extend(text("productId")),
        "customers" => {
            scope.customer_keys.insert(rec.key.clone());
        }
        "invoices" => {
            scope.invoice_keys.insert(rec.key.clone());
            scope.customer_keys.extend(text("customerKey"));
        }
        "payments" => scope.invoice_keys.extend(text("invoiceKey")),
        "creditNotes" => {
            scope.invoice_keys.extend(text("invoiceKey"));
            scope.customer_keys.extend(text("customerKey"));
        }
        _ => {}
    }
}

#[derive(Default)]
struct Counts {
    applied: u32,
    duplicates: u32,
    conflicts: u32,
}

/// Applies `records` (already permitted to write) inside `conn`'s transaction.
async fn apply_records_in_tx(
    db: &Db,
    conn: &mut SqliteConnection,
    records: &[&ChangeRecord],
    origin: &ApplyOrigin,
    scope: &mut DerivedScope,
    conflicts: &mut Vec<NewConflict>,
) -> AppResult<Counts> {
    let mut counts = Counts::default();
    let mut columns: HashMap<&'static str, Vec<Col>> = HashMap::new();
    let now_ms = Utc::now().timestamp_millis();

    // Parents before children; within an order, upserts before deletes.
    let mut ordered: Vec<&ChangeRecord> = records.to_vec();
    ordered.sort_by(|a, b| {
        let oa = resources::spec(&a.resource)
            .map(|s| s.order)
            .unwrap_or(u8::MAX);
        let ob = resources::spec(&b.resource)
            .map(|s| s.order)
            .unwrap_or(u8::MAX);
        (oa, a.op == ChangeOp::Delete, &a.key).cmp(&(ob, b.op == ChangeOp::Delete, &b.key))
    });

    for rec in ordered {
        let Some(spec) = resources::spec(&rec.resource) else {
            counts.conflicts += 1;
            continue;
        };
        let (ms, clamped) = clamp_updated_at(rec.updated_at.timestamp_millis(), now_ms);
        let incoming = IncomingChange {
            record: rec,
            updated_at_ms: ms,
            clamped,
        };
        let meta = load_meta(&mut *conn, spec.name, &rec.key).await?;
        let mut decision = decide(spec, meta.as_ref(), &incoming);

        // Append-only: identical repeats are duplicates, a different body under
        // the same key is refused.
        if spec.class == MergeClass::AppendOnly
            && meta.is_some()
            && matches!(decision, Decision::Duplicate)
            && let Some(local) = local_payload(db, spec, &rec.key).await?
            && let Some(payload) = &rec.payload
            && append_only_differs(spec, &local, payload)
        {
            decision = Decision::Reject {
                reason: "IMMUTABLE_MISMATCH",
            };
        }

        let device = match origin {
            ApplyOrigin::Device(id) => id.as_str(),
            ApplyOrigin::Cloud => rec.device_id.as_str(),
        };

        match decision {
            Decision::Duplicate => counts.duplicates += 1,
            Decision::Reject { reason } => {
                tracing::warn!(resource = spec.name, key = %rec.key, reason, "sync change rejected");
                counts.conflicts += 1;
            }
            Decision::KeepLocal { loser } => {
                counts.conflicts += 1;
                if let (Some(local), Some(losing)) =
                    (local_payload(db, spec, &rec.key).await?, &rec.payload)
                    && is_meaningful_conflict(spec, &local, losing)
                {
                    conflicts.push(NewConflict {
                        kind: loser,
                        resource: spec.name.to_string(),
                        entity_key: rec.key.clone(),
                        detail: serde_json::json!({
                            "losingPayload": without_secrets(spec.name, losing),
                            "winningDeviceId": meta.as_ref().map(|m| m.device_id.clone()),
                            "winningUpdatedAt": meta.as_ref().map(|m| ms_to_datetime(m.updated_at_ms)),
                        }),
                    });
                }
            }
            Decision::Apply | Decision::MergeLifecycle => {
                let merged_payload;
                let payload_ref: Option<&Value> = if matches!(decision, Decision::MergeLifecycle) {
                    let (Some(local), Some(incoming_payload)) =
                        (local_payload(db, spec, &rec.key).await?, &rec.payload)
                    else {
                        counts.conflicts += 1;
                        continue;
                    };
                    let outcome = merge_lifecycle(spec.name, &local, incoming_payload);
                    if outcome.rejected.is_some() {
                        counts.conflicts += 1;
                        continue;
                    }
                    if !outcome.changed {
                        counts.duplicates += 1;
                        continue;
                    }
                    merged_payload = outcome.merged;
                    Some(&merged_payload)
                } else {
                    rec.payload.as_ref()
                };

                let table_cols = match columns.get(spec.table) {
                    Some(c) => c.clone(),
                    None => {
                        let c = table_columns(&mut *conn, spec.table).await?;
                        columns.insert(spec.table, c.clone());
                        c
                    }
                };
                let meta_ms = match (&decision, &meta) {
                    (Decision::MergeLifecycle, Some(m)) => m.updated_at_ms.max(ms),
                    _ => ms,
                };
                let meta_device = match (&decision, &meta) {
                    (Decision::MergeLifecycle, Some(m)) if m.updated_at_ms > ms => {
                        m.device_id.as_str()
                    }
                    _ => rec.device_id.as_str(),
                };

                match rec.op {
                    ChangeOp::Delete if HARD_DELETE_TABLES.contains(&spec.table) => {
                        if spec.table == "categories" {
                            sqlx::query("DELETE FROM subcategories WHERE category_key = ?")
                                .bind(&rec.key)
                                .execute(&mut *conn)
                                .await?;
                        }
                        sqlx::query(&format!("DELETE FROM {} WHERE key = ?", spec.table))
                            .bind(&rec.key)
                            .execute(&mut *conn)
                            .await?;
                    }
                    ChangeOp::Delete => {
                        sqlx::query(&format!(
                            "UPDATE {t} SET deleted_at = ?, updated_at = ?, version = MAX(version, ?), updated_by_device = ? WHERE key = ?",
                            t = spec.table
                        ))
                        .bind(ms_to_datetime(ms).to_rfc3339())
                        .bind(ms_to_datetime(ms).to_rfc3339())
                        .bind(rec.version)
                        .bind(device)
                        .bind(&rec.key)
                        .execute(&mut *conn)
                        .await?;
                    }
                    ChangeOp::Upsert => {
                        let Some(Value::Object(payload)) = payload_ref else {
                            counts.conflicts += 1;
                            continue;
                        };
                        if spec.name == "productSerials"
                            && let Some(conflict) =
                                serial_conflict(&mut *conn, &rec.key, payload).await?
                        {
                            conflicts.push(conflict);
                        }
                        let write = RowWrite {
                            table: spec.table,
                            key: &rec.key,
                            payload,
                            ms: meta_ms,
                            version: rec.version,
                            device,
                            derived: spec.derived,
                        };
                        match upsert_with_unique_retry(
                            &mut *conn,
                            &table_cols,
                            &write,
                            spec.name,
                            &device_tag(&rec.device_id),
                        )
                        .await
                        {
                            Ok(fix) => {
                                if let Some(fix) = fix {
                                    conflicts.push(NewConflict {
                                        kind: ConflictKind::UniqueViolation,
                                        resource: spec.name.to_string(),
                                        entity_key: fix.entity_key,
                                        detail: serde_json::json!({
                                            "column": fix.column,
                                            "originalValue": fix.original,
                                            "storedValue": fix.stored,
                                        }),
                                    });
                                }
                            }
                            Err(err) => {
                                tracing::warn!(resource = spec.name, key = %rec.key, error = %err, "sync change could not be stored");
                                counts.conflicts += 1;
                                continue;
                            }
                        }
                        if spec.name == "users"
                            && payload.get("role").and_then(Value::as_str) == Some("admin")
                            && let Some(conflict) =
                                extra_admin_conflict(&mut *conn, &rec.key, conflicts).await?
                        {
                            conflicts.push(conflict);
                        }
                        if spec.name == "categories" {
                            reconcile_subcategories(&mut *conn, &rec.key, payload, ms, device)
                                .await?;
                        }
                    }
                }

                save_meta(
                    &mut *conn,
                    spec.name,
                    &rec.key,
                    meta_ms,
                    meta_device,
                    rec.version,
                )
                .await?;
                add_scope(spec, rec, scope);
                counts.applied += 1;
            }
        }
    }
    Ok(counts)
}

/// The one-admin-per-shop rule is enforced only by the users service create
/// path, which sync bypasses. When two DIFFERENT admins arrive for one shop
/// (created on two devices), both are kept - dropping either would lock a real
/// owner out - and the shop owner is told through a conflict. There is no
/// dedicated conflict kind, so this reuses `UniqueViolation` with
/// `reason = MULTIPLE_ADMINS` (the value that is "unique" is the admin role).
async fn extra_admin_conflict(
    conn: &mut SqliteConnection,
    key: &str,
    raised: &[NewConflict],
) -> AppResult<Option<NewConflict>> {
    let admin_keys: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM users WHERE role = 'admin' AND deleted_at IS NULL ORDER BY key",
    )
    .fetch_all(&mut *conn)
    .await?;
    if admin_keys.len() < 2 || !admin_keys.iter().any(|k| k == key) {
        return Ok(None);
    }
    let same_batch = raised
        .iter()
        .any(|c| c.resource == "users" && c.entity_key == key);
    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_conflicts WHERE kind = ? AND resource = 'users' AND entity_key = ? \
         AND resolved_at IS NULL AND detail LIKE '%MULTIPLE_ADMINS%'",
    )
    .bind(ConflictKind::UniqueViolation.as_str())
    .bind(key)
    .fetch_one(&mut *conn)
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

/// A serial claimed by two different invoices is kept as recorded on both
/// sides; the owner is told (the earlier sale stays the serial's state).
async fn serial_conflict(
    conn: &mut SqliteConnection,
    key: &str,
    incoming: &Map<String, Value>,
) -> AppResult<Option<NewConflict>> {
    let Some(local) =
        sqlx::query("SELECT serial_number, invoice_key, status FROM product_serials WHERE key = ?")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?
    else {
        return Ok(None);
    };
    let local_invoice: Option<String> = local.get("invoice_key");
    let incoming_invoice = incoming.get("invoiceKey").and_then(Value::as_str);
    match (local_invoice, incoming_invoice) {
        (Some(a), Some(b)) if a != b && local.get::<String, _>("status") == "sold" => {
            Ok(Some(NewConflict {
                kind: ConflictKind::SerialDoubleSold,
                resource: "productSerials".to_string(),
                entity_key: key.to_string(),
                detail: serde_json::json!({
                    "serialNumber": local.get::<String, _>("serial_number"),
                    "invoiceKeys": [a, b],
                }),
            }))
        }
        _ => Ok(None),
    }
}

/// A category travels with its subcategories: upsert the listed ones and
/// tombstone local ones the category no longer lists.
async fn reconcile_subcategories(
    conn: &mut SqliteConnection,
    category_key: &str,
    payload: &Map<String, Value>,
    ms: i64,
    device: &str,
) -> AppResult<()> {
    let cols = table_columns(&mut *conn, "subcategories").await?;
    let mut listed: HashSet<String> = HashSet::new();
    if let Some(Value::Array(subs)) = payload.get("subcategories") {
        for sub in subs {
            let Value::Object(sub) = sub else { continue };
            let Some(key) = sub.get("key").and_then(Value::as_str) else {
                continue;
            };
            listed.insert(key.to_string());
            let mut fields = sub.clone();
            fields.insert(
                "categoryKey".to_string(),
                Value::String(category_key.to_string()),
            );
            let version = sub.get("version").and_then(Value::as_i64).unwrap_or(1);
            let write = RowWrite {
                table: "subcategories",
                key,
                payload: &fields,
                ms,
                version,
                device,
                derived: &[],
            };
            upsert_row(&mut *conn, &cols, &write).await?;
        }
    }
    // Subcategories are removed for real locally, so the category's list is the
    // whole truth: anything it no longer lists goes.
    let existing: Vec<String> =
        sqlx::query_scalar("SELECT key FROM subcategories WHERE category_key = ?")
            .bind(category_key)
            .fetch_all(&mut *conn)
            .await?;
    for key in existing.into_iter().filter(|k| !listed.contains(k)) {
        sqlx::query("DELETE FROM subcategories WHERE key = ?")
            .bind(&key)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Empties every synced table (first page of a bootstrap).
async fn wipe_synced_tables(conn: &mut SqliteConnection) -> AppResult<()> {
    for spec in SYNC_RESOURCES
        .iter()
        .filter(|s| s.phase == Phase::P3a)
        .rev()
    {
        sqlx::query(&format!("DELETE FROM {}", spec.table))
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("DELETE FROM subcategories")
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM sync_outbox")
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM sync_row_meta")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A device that joined an existing shop by downloading it has no local "first
/// run" left to do: the shop's admin arrived with the snapshot. Marks the
/// installation as set up so the POS shows its sign-in instead of the wizard's
/// admin form. Runs on the final bootstrap page, in the same transaction as the
/// cursor, so a half-downloaded shop is never marked as ready. Does nothing
/// when the snapshot carries no active admin (the owner then sets one up here).
async fn adopt_cloud_setup(tx: &mut sqlx::SqliteConnection) -> AppResult<()> {
    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users \
         WHERE role = 'admin' AND is_active = 1 AND deleted_at IS NULL",
    )
    .fetch_one(&mut *tx)
    .await?;
    if admins == 0 {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    // The snapshot replaced any demo rows, so the sample-data flag no longer holds.
    let updated = sqlx::query(
        "UPDATE system_installations \
         SET setup_completed = 1, sample_data_loaded = 0, \
             setup_completed_at = COALESCE(setup_completed_at, ?1), updated_at = ?1",
    )
    .bind(&now)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 0 {
        // No installation row yet (setup was never opened): create it, already complete.
        let installation_id = std::env::var("INSTALLATION_ID")
            .unwrap_or_else(|_| format!("inst_{}", nanoid::nanoid!(16)));
        let app_version = std::env::var("APP_VERSION").unwrap_or_else(|_| "0.5.0".to_string());
        let platform =
            std::env::var("PLATFORM").unwrap_or_else(|_| std::env::consts::OS.to_string());
        sqlx::query(
            "INSERT INTO system_installations \
             (key, id, installation_id, app_version, platform, installed_at, setup_completed, \
              setup_completed_at, sample_data_loaded, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?6, 0, ?6, ?6)",
        )
        .bind(format!("inst_{}", nanoid::nanoid!(16)))
        .bind(format!("inst_{}", nanoid::nanoid!(16)))
        .bind(installation_id)
        .bind(app_version)
        .bind(platform)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// `POST /api/sync/apply`: one page of pulled changes, in one transaction.
pub async fn apply_batch(db: &Db, req: ApplyRequest) -> AppResult<ApplyResponse> {
    crate::core::logging::domain::tracked("sync.applied", async move {
        let pool = pool_of(db)?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("UPDATE sync_state SET applying = 1 WHERE id = 1")
            .execute(&mut *tx)
            .await?;

        if req.mode == ApplyMode::Bootstrap {
            let active: i64 =
                sqlx::query_scalar("SELECT bootstrap_active FROM sync_state WHERE id = 1")
                    .fetch_one(&mut *tx)
                    .await?;
            if active == 0 {
                wipe_synced_tables(&mut tx).await?;
                sqlx::query("UPDATE sync_state SET bootstrap_active = 1 WHERE id = 1")
                    .execute(&mut *tx)
                    .await?;
            }
        }

        let records: Vec<&ChangeRecord> = req.changes.iter().map(|c| &c.record).collect();
        let mut scope = DerivedScope::default();
        let mut conflicts = Vec::new();
        let counts = apply_records_in_tx(
            db,
            &mut tx,
            &records,
            &ApplyOrigin::Cloud,
            &mut scope,
            &mut conflicts,
        )
        .await?;

        conflicts.extend(derived_sqlite::recompute(&mut tx, &scope).await?);
        derived_sqlite::store_conflicts(&mut tx, &conflicts).await?;

        if let Some(cursor) = req.advance_cursor_to {
            sqlx::query(
                "UPDATE sync_state SET cloud_cursor = ?, bootstrap_active = 0 WHERE id = 1",
            )
            .bind(cursor)
            .execute(&mut *tx)
            .await?;
            if req.mode == ApplyMode::Bootstrap {
                adopt_cloud_setup(&mut tx).await?;
            }
        }
        // Cleared inside the same transaction: a rollback reverts it too.
        sqlx::query("UPDATE sync_state SET applying = 0 WHERE id = 1")
            .execute(&mut *tx)
            .await?;
        let cursor: i64 = sqlx::query_scalar("SELECT cloud_cursor FROM sync_state WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(ApplyResponse {
            applied: counts.applied,
            duplicates: counts.duplicates,
            conflicts: counts.conflicts + conflicts.len() as u32,
            cursor,
        })
    })
    .await
}

/// SQLite arm of the per-module `apply_sync_documents` functions: applies the
/// given records in their own transaction and reports what happened.
pub async fn apply_records(
    db: &Db,
    pool: &SqlitePool,
    changes: &[&ChangeRecord],
    origin: ApplyOrigin,
) -> AppResult<ApplyOutcome> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("UPDATE sync_state SET applying = 1 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let mut scope = DerivedScope::default();
    let mut conflicts = Vec::new();
    let counts =
        apply_records_in_tx(db, &mut tx, changes, &origin, &mut scope, &mut conflicts).await?;
    conflicts.extend(derived_sqlite::recompute(&mut tx, &scope).await?);
    derived_sqlite::store_conflicts(&mut tx, &conflicts).await?;
    sqlx::query("UPDATE sync_state SET applying = 0 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ApplyOutcome {
        applied: counts.applied,
        duplicates: counts.duplicates,
        conflicts,
        touched: scope,
    })
}
