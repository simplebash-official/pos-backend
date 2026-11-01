use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::TENANT_FIELD;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::UpdateOptions;
use std::collections::HashMap;

/// The balance one user ends up with.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedDrawer {
    /// `None` on a single-shop deployment.
    pub tenant_id: Option<String>,
    pub user_key: String,
    pub balance_cents: i64,
    /// The machine whose row was kept.
    pub from_terminal_id: String,
}

/// What a run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct Report {
    /// Old per-machine rows found.
    pub machine_rows: usize,
    /// One per user, the most recently updated of their machines' rows.
    pub planned: Vec<PlannedDrawer>,
    /// Rows of machines that never had a shift, so belong to nobody; dropped.
    pub ownerless: Vec<String>,
}

fn text(document: &Document, field: &str) -> AppResult<String> {
    document
        .get_str(field)
        .map(str::to_string)
        .map_err(|e| AppError::internal(format!("drawer_state.{field}: {e}")))
}

/// The cashier of the row's last shift, else of the latest shift on that machine.
async fn owner_of(
    sessions: &mongodb::Collection<Document>,
    row: &Document,
) -> AppResult<Option<String>> {
    if let Ok(key) = row.get_str("last_session_key")
        && let Some(shift) = sessions.find_one(doc! { "key": key }).await?
    {
        return Ok(Some(text(&shift, "cashier_id")?));
    }
    let mut filter = doc! { "terminal_id": text(row, "terminal_id")? };
    if let Ok(tenant) = row.get_str(TENANT_FIELD) {
        filter.insert(TENANT_FIELD, tenant);
    }
    let latest = sessions
        .find_one(filter)
        .sort(doc! { "opened_at": -1 })
        .await?;
    latest.map(|shift| text(&shift, "cashier_id")).transpose()
}

/// Rewrites every shop's per-machine drawer rows as one row per user.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> AppResult<Report> {
    let drawers = db.collection::<Document>("drawer_state");
    let sessions = db.collection::<Document>("register_sessions");
    let rows: Vec<Document> = drawers
        .find(doc! { "terminal_id": { "$exists": true } })
        .await?
        .try_collect()
        .await?;

    let mut report = Report {
        machine_rows: rows.len(),
        ..Report::default()
    };
    let mut newest: HashMap<(Option<String>, String), Document> = HashMap::new();
    for row in &rows {
        let Some(owner) = owner_of(&sessions, row).await? else {
            report.ownerless.push(text(row, "terminal_id")?);
            continue;
        };
        let tenant = row.get_str(TENANT_FIELD).ok().map(str::to_string);
        let updated = |d: &Document| d.get_datetime("updated_at").copied().ok();
        let slot = newest.entry((tenant, owner)).or_insert_with(|| row.clone());
        if updated(row) > updated(slot) {
            *slot = row.clone();
        }
    }

    for ((tenant_id, user_key), row) in newest {
        let balance_cents = row
            .get_i64("current_balance_cents")
            .map_err(|e| AppError::internal(format!("drawer_state.current_balance_cents: {e}")))?;
        if mode == Mode::Apply {
            let mut filter = doc! { "user_key": &user_key };
            if let Some(tenant) = &tenant_id {
                filter.insert(TENANT_FIELD, tenant);
            }
            let mut fresh = filter.clone();
            fresh.insert("current_balance_cents", balance_cents);
            if let Ok(key) = row.get_str("last_session_key") {
                fresh.insert("last_session_key", key);
            }
            fresh.insert(
                "last_closing_float_cents",
                row.get_i64("last_closing_float_cents").map_err(|e| {
                    AppError::internal(format!("drawer_state.last_closing_float_cents: {e}"))
                })?,
            );
            fresh.insert(
                "updated_at",
                *row.get_datetime("updated_at")
                    .map_err(|e| AppError::internal(format!("drawer_state.updated_at: {e}")))?,
            );
            // A row the new code already wrote for this user is newer than any machine row.
            drawers
                .update_one(filter, doc! { "$setOnInsert": fresh })
                .with_options(UpdateOptions::builder().upsert(true).build())
                .await?;
        }
        report.planned.push(PlannedDrawer {
            tenant_id,
            user_key,
            balance_cents,
            from_terminal_id: text(&row, "terminal_id")?,
        });
    }
    if mode == Mode::Apply {
        drawers
            .delete_many(doc! { "terminal_id": { "$exists": true } })
            .await?;
    }
    report
        .planned
        .sort_by(|a, b| (&a.tenant_id, &a.user_key).cmp(&(&b.tenant_id, &b.user_key)));
    Ok(report)
}
