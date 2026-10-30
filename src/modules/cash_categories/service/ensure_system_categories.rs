use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::cash_categories::{ReviewStatus, category_key, normalize_name};
use crate::modules::cash_categories::model::CashCategoryDocument;
use crate::modules::cash_categories::repository;
use crate::seeds::cash_categories::{CashCategorySeedResult, SYSTEM_CATEGORIES};
use mongodb::bson::DateTime as BsonDateTime;
use std::collections::HashMap;

/// 2026-01-01T00:00:00Z: the fixed time built-in categories are stamped with (see `ensure_system_categories`).
const SEEDED_AT_MS: i64 = 1_767_225_600_000;

/// Creates any shipped category this shop is missing. Never touches an existing row, so edits survive.
pub async fn ensure_system_categories(db: &Db) -> AppResult<CashCategorySeedResult> {
    let mut created = 0;
    let mut existing = 0;
    let mut next_sort: HashMap<&'static str, i64> = HashMap::new();
    for (movement_type, name, report_group) in SYSTEM_CATEGORIES {
        let sort = next_sort.entry(movement_type.as_str()).or_insert(0);
        let sort_order = *sort;
        *sort += 1;
        let key = category_key(*movement_type, name);
        if repository::find_by_key(db, &key).await?.is_some() {
            existing += 1;
            continue;
        }
        // Every device and the cloud seed the same row with the same fixed time, so when sync
        // compares them (last writer wins) any real edit by the shop is newer and a re-seed
        // can never undo a rename made elsewhere.
        let seeded_at = BsonDateTime::from_millis(SEEDED_AT_MS);
        repository::insert(
            db,
            CashCategoryDocument {
                id: None,
                key,
                movement_type: *movement_type,
                name: (*name).to_string(),
                name_si: None,
                normalized_name: normalize_name(name),
                report_group: *report_group,
                is_system: true,
                review_status: ReviewStatus::Approved,
                pinned: false,
                sort_order,
                active: true,
                requires_receipt: false,
                requires_approval_over_cents: None,
                merged_into_key: None,
                created_by_name_snapshot: None,
                version: 1,
                created_at: seeded_at,
                updated_at: seeded_at,
                updated_by_device: None,
            },
        )
        .await?;
        created += 1;
    }
    // The drawer rules are the same kind of required starting data, and every shop passes through here.
    crate::modules::cash_settings::service::ensure_settings(db).await?;
    Ok(CashCategorySeedResult { created, existing })
}
