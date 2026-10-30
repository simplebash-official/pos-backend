use super::clean_name::clean_name;
use super::clean_name_si::clean_name_si;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::cash_categories::{
    CashCategory, CreateCashCategoryRequest, ReportGroup, ReviewStatus, category_key,
    normalize_name,
};
use crate::domain::sessions::CashMovementType;
use crate::modules::cash_categories::model::CashCategoryDocument;
use crate::modules::cash_categories::repository;
use mongodb::bson::DateTime as BsonDateTime;

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
