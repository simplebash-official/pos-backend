// Business rules for drawer cash categories: who may create what, unique names per
// movement type, similar-name warnings, merging, hiding and the seeded starting list.

use std::collections::HashMap;

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::codes,
        error::{AppError, AppResult},
        logging::domain::tracked,
    },
    domain::{
        cash_categories::{
            CashCategory, CashCategoryListQuery, CreateCashCategoryRequest,
            MergeCashCategoryRequest, ReportGroup, ReviewStatus, SimilarCashCategoryQuery,
            UpdateCashCategoryRequest, category_key, normalize_name,
        },
        sessions::CashMovementType,
    },
    modules::{
        cash_categories::{model::CashCategoryDocument, repository},
        sessions,
    },
    seeds::cash_categories::{CashCategorySeedResult, SYSTEM_CATEGORIES},
};

const NAME_MIN: usize = 2;
const NAME_MAX: usize = 40;
/// 2026-01-01T00:00:00Z: the fixed time built-in categories are stamped with (see `ensure_system_categories`).
const SEEDED_AT_MS: i64 = 1_767_225_600_000;
/// The one name every type's catch-all category carries; one-off labels file under it.
pub const OTHER_NAME: &str = "Other";

/// Types a cashier (no `cash_categories:manage`) may add categories for. The rest move
/// accountable money and need a manager.
const CASHIER_TYPES: &[CashMovementType] = &[
    CashMovementType::PettyExpense,
    CashMovementType::AddCash,
    CashMovementType::CashDrop,
    CashMovementType::NoSale,
];

/// The report group a new category inherits from its movement type.
fn inherited_report_group(movement_type: CashMovementType) -> ReportGroup {
    match movement_type {
        CashMovementType::PettyExpense | CashMovementType::RentAndBills => {
            ReportGroup::OperatingExpense
        }
        CashMovementType::StaffPayment => ReportGroup::Payroll,
        CashMovementType::SupplierPayment => ReportGroup::Purchases,
        CashMovementType::OwnerWithdrawal => ReportGroup::OwnerEquity,
        CashMovementType::AddCash | CashMovementType::CashDrop | CashMovementType::NoSale => {
            ReportGroup::Transfer
        }
    }
}

/// Key of the type's "Other" category, where one-off labels and migrated movements are filed.
pub fn other_category_key(movement_type: CashMovementType) -> String {
    category_key(movement_type, OTHER_NAME)
}

fn clean_name(raw: &str) -> AppResult<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let len = name.chars().count();
    if !(NAME_MIN..=NAME_MAX).contains(&len) {
        return Err(AppError::validation(format!(
            "A category name needs {NAME_MIN} to {NAME_MAX} letters."
        )));
    }
    Ok(name)
}

fn clean_name_si(raw: &str) -> AppResult<Option<String>> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Ok(None);
    }
    if name.chars().count() > NAME_MAX {
        return Err(AppError::validation(format!(
            "The Sinhala name can have up to {NAME_MAX} letters."
        )));
    }
    Ok(Some(name))
}

async fn usage_by_key(db: &Db) -> AppResult<HashMap<String, i64>> {
    Ok(sessions::service::movement_counts_by_category(db)
        .await?
        .into_iter()
        .collect())
}

/// Usage of a category including everything merged into it.
fn usage_of(
    doc: &CashCategoryDocument,
    all: &[CashCategoryDocument],
    usage: &HashMap<String, i64>,
) -> i64 {
    let own = usage.get(&doc.key).copied().unwrap_or(0);
    let merged: i64 = all
        .iter()
        .filter(|c| c.merged_into_key.as_deref() == Some(doc.key.as_str()))
        .map(|c| usage.get(&c.key).copied().unwrap_or(0))
        .sum();
    own + merged
}

/// Categories for the picker and the management screen, best first: pinned, then most
/// used by this shop, then the shop's own order. Merged-away categories never appear.
pub async fn list_categories(
    db: &Db,
    query: CashCategoryListQuery,
) -> AppResult<Vec<CashCategory>> {
    let movement_type = query
        .movement_type
        .as_deref()
        .map(|t| {
            CashMovementType::parse(t).ok_or_else(|| AppError::validation("Unknown movement type."))
        })
        .transpose()?;
    let include_hidden = query.include_hidden.unwrap_or(false);
    let needs_review = query.needs_review.unwrap_or(false);
    let only_costs = query.for_expenses.unwrap_or(false);
    let only_income = query.for_income.unwrap_or(false);
    let search = query
        .search
        .as_deref()
        .map(normalize_name)
        .filter(|s| !s.is_empty());

    let all = repository::list_all(db).await?;
    let usage = usage_by_key(db).await?;

    let mut out: Vec<CashCategory> = all
        .iter()
        .filter(|c| c.merged_into_key.is_none())
        .filter(|c| movement_type.is_none_or(|t| c.movement_type == t))
        .filter(|c| include_hidden || c.active)
        .filter(|c| !needs_review || c.review_status == ReviewStatus::NeedsReview)
        .filter(|c| !only_costs || is_expense_group(c.report_group))
        .filter(|c| !only_income || c.report_group == ReportGroup::OtherIncome)
        .filter(|c| {
            search.as_ref().is_none_or(|q| {
                c.normalized_name.contains(q.as_str())
                    || c.name_si
                        .as_deref()
                        .is_some_and(|si| normalize_name(si).contains(q.as_str()))
            })
        })
        .cloned()
        .map(|c| {
            let n = usage_of(&c, &all, &usage);
            c.into_cash_category(n)
        })
        .collect();
    out.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(b.usage_count.cmp(&a.usage_count))
            .then(a.sort_order.cmp(&b.sort_order))
            .then(a.name.cmp(&b.name))
    });
    Ok(out)
}

/// Creates a category. Cashiers may only add to the low-risk types and their category waits
/// for review; managers may add to any type and choose its report group.
pub async fn create_category(
    db: &Db,
    created_by_name: String,
    can_manage: bool,
    body: CreateCashCategoryRequest,
) -> AppResult<CashCategory> {
    tracked("cash_categories.created", async move {
        let name = clean_name(&body.name)?;
        let name_si = body.name_si.as_deref().map(clean_name_si).transpose()?.flatten();
        if !can_manage && !CASHIER_TYPES.contains(&body.movement_type) {
            return Err(AppError::forbidden_with_code(
                "Ask a manager to add a category for this kind of payment.",
                codes::CASH_CATEGORY_TYPE_RESTRICTED,
            ));
        }
        if body.report_group.is_some() && !can_manage {
            return Err(AppError::forbidden_with_code(
                "Only a manager can choose how a category is reported.",
                codes::PERMISSION_DENIED,
            ));
        }

        let normalized = normalize_name(&name);
        let all = repository::list_all(db).await?;
        if let Some(existing) = all
            .iter()
            .find(|c| c.movement_type == body.movement_type && c.normalized_name == normalized)
        {
            return Err(if existing.active && existing.merged_into_key.is_none() {
                AppError::conflict(
                    codes::CASH_CATEGORY_NAME_TAKEN,
                    "A category with this name already exists.",
                )
            } else {
                AppError::conflict(
                    codes::CASH_CATEGORY_HIDDEN,
                    "A category with this name exists but is hidden. Show it again instead of adding it.",
                )
            });
        }
        let key = category_key(body.movement_type, &name);
        if repository::find_by_key(db, &key).await?.is_some() {
            return Err(AppError::conflict(
                codes::CASH_CATEGORY_NAME_TAKEN,
                "This name was used by a category that has since been renamed. Please choose a different name.",
            ));
        }

        let now = BsonDateTime::now();
        let sort_order = all
            .iter()
            .filter(|c| c.movement_type == body.movement_type)
            .map(|c| c.sort_order)
            .max()
            .map_or(0, |m| m + 1);
        let doc = CashCategoryDocument {
            id: None,
            key,
            movement_type: body.movement_type,
            name,
            name_si,
            normalized_name: normalized,
            report_group: body
                .report_group
                .unwrap_or_else(|| inherited_report_group(body.movement_type)),
            is_system: false,
            review_status: if can_manage { ReviewStatus::Approved } else { ReviewStatus::NeedsReview },
            pinned: false,
            sort_order,
            active: true,
            requires_receipt: false,
            requires_approval_over_cents: None,
            merged_into_key: None,
            created_by_name_snapshot: Some(created_by_name),
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        };
        let inserted = repository::insert(db, doc).await?;
        Ok(inserted.into_cash_category(0))
    })
    .await
}

async fn require_category(db: &Db, key: &str) -> AppResult<CashCategoryDocument> {
    repository::find_by_key(db, key).await?.ok_or_else(|| {
        AppError::not_found_with_code("Category not found.", codes::CASH_CATEGORY_NOT_FOUND)
    })
}

/// Renames, hides, pins, reorders, sets receipt/approval rules on, or approves a category.
pub async fn update_category(
    db: &Db,
    key: &str,
    body: UpdateCashCategoryRequest,
) -> AppResult<CashCategory> {
    tracked("cash_categories.updated", async move {
        let mut doc = require_category(db, key).await?;
        if doc.merged_into_key.is_some() {
            return Err(AppError::conflict(
                codes::CASH_CATEGORY_HIDDEN,
                "This category was merged into another one and can no longer be changed.",
            ));
        }
        let is_other = doc.key == other_category_key(doc.movement_type);

        if let Some(raw) = &body.name {
            let name = clean_name(raw)?;
            let normalized = normalize_name(&name);
            if is_other && normalized != doc.normalized_name {
                return Err(AppError::validation(
                    "The Other category keeps its name, because one-off labels are filed under it.",
                ));
            }
            if repository::list_all(db).await?.iter().any(|c| {
                c.key != doc.key && c.movement_type == doc.movement_type && c.normalized_name == normalized
            }) {
                return Err(AppError::conflict(
                    codes::CASH_CATEGORY_NAME_TAKEN,
                    "A category with this name already exists.",
                ));
            }
            doc.name = name;
            doc.normalized_name = normalized;
        }
        if let Some(raw) = &body.name_si {
            doc.name_si = clean_name_si(raw)?;
        }
        if let Some(group) = body.report_group {
            doc.report_group = group;
        }
        if let Some(pinned) = body.pinned {
            doc.pinned = pinned;
        }
        if let Some(sort_order) = body.sort_order {
            doc.sort_order = sort_order;
        }
        if let Some(active) = body.active {
            if !active && is_other {
                return Err(AppError::validation(
                    "The Other category cannot be hidden, because one-off labels are filed under it.",
                ));
            }
            doc.active = active;
        }
        if let Some(required) = body.requires_receipt {
            doc.requires_receipt = required;
        }
        if let Some(limit) = body.requires_approval_over_cents {
            if limit < 0 {
                return Err(AppError::validation("The approval limit cannot be negative."));
            }
            doc.requires_approval_over_cents = (limit > 0).then_some(limit);
        }
        if body.approve == Some(true) {
            doc.review_status = ReviewStatus::Approved;
        }

        doc.version += 1;
        doc.updated_at = BsonDateTime::now();
        repository::save(db, &doc).await?;
        let all = repository::list_all(db).await?;
        let usage = usage_by_key(db).await?;
        let n = usage_of(&doc, &all, &usage);
        Ok(doc.into_cash_category(n))
    })
    .await
}

/// Moves a duplicate into another category of the same type. Movements are append-only, so
/// they keep their key; the merged category redirects readers to the target.
pub async fn merge_category(
    db: &Db,
    key: &str,
    body: MergeCashCategoryRequest,
) -> AppResult<CashCategory> {
    tracked("cash_categories.merged", async move {
        if key == body.target_key {
            return Err(AppError::validation(
                "Pick a different category to merge into.",
            ));
        }
        let mut source = require_category(db, key).await?;
        let target = require_category(db, &body.target_key).await?;
        if source.is_system {
            return Err(AppError::validation(
                "A built-in category cannot be merged away. You can hide it instead.",
            ));
        }
        if source.movement_type != target.movement_type {
            return Err(AppError::validation_with_code(
                "Categories can only be merged within the same kind of payment.",
                codes::CASH_CATEGORY_WRONG_TYPE,
            ));
        }
        if !target.active || target.merged_into_key.is_some() {
            return Err(AppError::conflict(
                codes::CASH_CATEGORY_HIDDEN,
                "The category to merge into is hidden.",
            ));
        }
        if source.merged_into_key.is_some() {
            return Err(AppError::conflict(
                codes::CASH_CATEGORY_HIDDEN,
                "This category was already merged.",
            ));
        }

        let now = BsonDateTime::now();
        // Anything already merged into the source now points straight at the target.
        for mut earlier in repository::list_all(db)
            .await?
            .into_iter()
            .filter(|c| c.merged_into_key.as_deref() == Some(source.key.as_str()))
        {
            earlier.merged_into_key = Some(target.key.clone());
            earlier.version += 1;
            earlier.updated_at = now;
            repository::save(db, &earlier).await?;
        }
        source.merged_into_key = Some(target.key.clone());
        source.active = false;
        source.version += 1;
        source.updated_at = now;
        repository::save(db, &source).await?;

        let all = repository::list_all(db).await?;
        let usage = usage_by_key(db).await?;
        let n = usage_of(&target, &all, &usage);
        Ok(target.into_cash_category(n))
    })
    .await
}

/// Edit distance, for catching typos like "Fule" against "Fuel".
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Whether two normalized names are close enough to ask "did you mean?".
fn names_are_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short.chars().count() >= 3 && long.contains(short) {
        return true;
    }
    short.chars().count() >= 4 && edit_distance(a, b) <= 2
}

/// Existing categories whose name is close to a typed one, so the cashier can use one instead of adding a near-duplicate.
pub async fn similar_categories(
    db: &Db,
    query: SimilarCashCategoryQuery,
) -> AppResult<Vec<CashCategory>> {
    let movement_type = CashMovementType::parse(&query.movement_type)
        .ok_or_else(|| AppError::validation("Unknown movement type."))?;
    let typed = normalize_name(&query.name);
    if typed.is_empty() {
        return Ok(Vec::new());
    }
    let all = repository::list_all(db).await?;
    let usage = usage_by_key(db).await?;
    let mut hits: Vec<CashCategory> = all
        .iter()
        .filter(|c| c.movement_type == movement_type && c.active && c.merged_into_key.is_none())
        .filter(|c| names_are_similar(&typed, &c.normalized_name))
        .cloned()
        .map(|c| {
            let n = usage_of(&c, &all, &usage);
            c.into_cash_category(n)
        })
        .collect();
    hits.sort_by_key(|c| std::cmp::Reverse(c.usage_count));
    hits.truncate(3);
    Ok(hits)
}

/// The category a new movement is filed under: it must exist, belong to the movement's type
/// and still be offered. Returns the document so the caller can read its approval rules.
pub(crate) async fn category_for_movement(
    db: &Db,
    key: &str,
    movement_type: CashMovementType,
) -> AppResult<CashCategoryDocument> {
    let doc = require_category(db, key).await?;
    if doc.movement_type != movement_type {
        return Err(AppError::validation_with_code(
            "This category belongs to a different kind of payment.",
            codes::CASH_CATEGORY_WRONG_TYPE,
        ));
    }
    if !doc.active || doc.merged_into_key.is_some() {
        return Err(AppError::conflict(
            codes::CASH_CATEGORY_HIDDEN,
            "This category is hidden. Please choose another one.",
        ));
    }
    Ok(doc)
}

/// What a category key stands for today: its live name and report group, following merges.
pub(crate) struct ResolvedCategory {
    pub name: String,
    pub report_group: ReportGroup,
}

/// Resolves category keys to their live name and report group, following merges, so a rename or
/// merge shows on old movements and a merged-away category reports as its target.
pub(crate) async fn resolve_categories(
    db: &Db,
    keys: &[String],
) -> AppResult<HashMap<String, ResolvedCategory>> {
    let all: HashMap<String, CashCategoryDocument> = repository::list_all(db)
        .await?
        .into_iter()
        .map(|c| (c.key.clone(), c))
        .collect();
    let mut out = HashMap::new();
    for key in keys {
        let mut current = all.get(key).ok_or_else(|| {
            AppError::internal(format!("A record points at the missing category '{key}'"))
        })?;
        while let Some(next) = current.merged_into_key.as_ref() {
            current = all.get(next).ok_or_else(|| {
                AppError::internal(format!(
                    "Category '{key}' was merged into the missing category '{next}'"
                ))
            })?;
        }
        out.insert(
            key.clone(),
            ResolvedCategory {
                name: current.name.clone(),
                report_group: current.report_group,
            },
        );
    }
    Ok(out)
}

/// Live display names for category keys (see [`resolve_categories`]).
pub(crate) async fn display_names(db: &Db, keys: &[String]) -> AppResult<HashMap<String, String>> {
    Ok(resolve_categories(db, keys)
        .await?
        .into_iter()
        .map(|(key, resolved)| (key, resolved.name))
        .collect())
}

/// Report groups whose costs belong in the expense ledger.
pub(crate) fn is_expense_group(group: ReportGroup) -> bool {
    matches!(
        group,
        ReportGroup::OperatingExpense | ReportGroup::Purchases | ReportGroup::Payroll
    )
}

/// A category an expense can be filed under: visible, and counted as a cost.
pub(crate) async fn category_for_expense(db: &Db, key: &str) -> AppResult<CashCategoryDocument> {
    let doc = require_category(db, key).await?;
    if !doc.active || doc.merged_into_key.is_some() {
        return Err(AppError::conflict(
            codes::CASH_CATEGORY_HIDDEN,
            "This category is hidden. Please choose another one.",
        ));
    }
    if !is_expense_group(doc.report_group) {
        return Err(AppError::validation_with_code(
            "This category is not a business cost. Please choose another one.",
            codes::CASH_CATEGORY_WRONG_TYPE,
        ));
    }
    Ok(doc)
}

/// A category an income can be filed under: visible, and counted as income.
pub(crate) async fn category_for_income(db: &Db, key: &str) -> AppResult<CashCategoryDocument> {
    let doc = require_category(db, key).await?;
    if !doc.active || doc.merged_into_key.is_some() {
        return Err(AppError::conflict(
            codes::CASH_CATEGORY_HIDDEN,
            "This category is hidden. Please choose another one.",
        ));
    }
    if doc.report_group != ReportGroup::OtherIncome {
        return Err(AppError::validation_with_code(
            "This category is not income. Please choose another one.",
            codes::CASH_CATEGORY_WRONG_TYPE,
        ));
    }
    Ok(doc)
}

/// `key` plus every category merged into it, for filtering movements by a category.
pub(crate) async fn keys_including_merged(db: &Db, key: &str) -> AppResult<Vec<String>> {
    let mut keys: Vec<String> = repository::list_all(db)
        .await?
        .into_iter()
        .filter(|c| c.merged_into_key.as_deref() == Some(key))
        .map(|c| c.key)
        .collect();
    keys.push(key.to_string());
    Ok(keys)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_names_catch_typos_and_containment() {
        assert!(names_are_similar("fuel", "fule"));
        assert!(names_are_similar("tea", "teaandrefreshments"));
        assert!(names_are_similar("fuel", "fuel"));
        assert!(!names_are_similar("fuel", "stationery"));
        assert!(!names_are_similar("ab", "xy"));
    }

    #[test]
    fn names_are_cleaned_and_bounded() {
        assert_eq!(
            clean_name("  Generator   diesel ").unwrap(),
            "Generator diesel"
        );
        assert!(clean_name("a").is_err());
        assert!(clean_name(&"x".repeat(41)).is_err());
        assert_eq!(clean_name_si("   ").unwrap(), None);
    }

    #[test]
    fn every_seeded_type_has_an_other_unless_it_is_a_drop() {
        for t in [
            CashMovementType::PettyExpense,
            CashMovementType::RentAndBills,
            CashMovementType::SupplierPayment,
            CashMovementType::OwnerWithdrawal,
            CashMovementType::AddCash,
            CashMovementType::NoSale,
        ] {
            assert!(
                SYSTEM_CATEGORIES
                    .iter()
                    .any(|(ty, n, _)| *ty == t && *n == OTHER_NAME),
                "{t:?} needs an Other category"
            );
        }
    }

    #[test]
    fn seeded_names_are_unique_per_type() {
        let mut seen = std::collections::HashSet::new();
        for (t, n, _) in SYSTEM_CATEGORIES {
            assert!(
                seen.insert((t.as_str(), normalize_name(n))),
                "duplicate {n} for {t:?}"
            );
        }
    }
}
