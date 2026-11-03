use super::warranty_events::{self, NewEvent, WarrantyActor};
use super::warranty_lookup::{detail, require};
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::warranties::{
    ExtendWarrantyRequest, VoidWarrantyRequest, WarrantyCoverage, WarrantyDetail,
    WarrantyEventKind, WarrantyStatus,
};
use crate::modules::users;
use crate::modules::warranties::model::stored_day;
use crate::modules::warranties::repository;
use mongodb::bson::DateTime as BsonDateTime;
use pos_calc_core::warranty::{self, WarrantyEnd};

fn version_conflict() -> AppError {
    AppError::conflict(
        codes::VERSION_CONFLICT,
        "Someone else changed this warranty just now. Open it again and retry.",
    )
}

/// Cancels a warranty with a manager's or owner's approval. A void warranty cannot be voided
/// again.
pub(crate) async fn void(
    db: &Db,
    key: &str,
    body: VoidWarrantyRequest,
    actor: &WarrantyActor,
) -> AppResult<WarrantyDetail> {
    crate::core::logging::domain::tracked("warranties.voided", async move {
        let mut warranty = require(db, key).await?;
        if warranty.status == WarrantyStatus::Void {
            return Err(AppError::conflict(
                codes::WARRANTY_ALREADY_VOID,
                "This warranty is already cancelled.",
            ));
        }
        let approver = users::service::require_manager_approval(db, &body.approval).await?;
        let from = warranty.status;
        let expected = warranty.version;
        warranty.status = WarrantyStatus::Void;
        warranty.void_reason = Some(approver.reason.clone());
        warranty.version += 1;
        warranty.updated_at = BsonDateTime::now();
        if !repository::replace_warranty(db, &warranty, expected).await? {
            return Err(version_conflict());
        }
        warranty_events::record(
            db,
            NewEvent {
                warranty_key: &warranty.key,
                claim_key: None,
                kind: WarrantyEventKind::Voided,
                from_status: Some(from.as_str()),
                to_status: Some(WarrantyStatus::Void.as_str()),
                note: Some(approver.reason.clone()),
                actor,
                approved_by: Some(&approver),
            },
        )
        .await?;
        detail(db, &warranty.key).await
    })
    .await
}

/// Gives a warranty a longer period, counted from its original first day, with a manager's
/// or owner's approval. The new period must end later than the current one; a lifetime
/// warranty cannot be extended and a void one cannot be changed.
pub(crate) async fn extend(
    db: &Db,
    key: &str,
    body: ExtendWarrantyRequest,
    actor: &WarrantyActor,
) -> AppResult<WarrantyDetail> {
    crate::core::logging::domain::tracked("warranties.extended", async move {
        let mut warranty = require(db, key).await?;
        if warranty.status == WarrantyStatus::Void {
            return Err(AppError::conflict(
                codes::WARRANTY_NOT_ACTIVE,
                "A cancelled warranty cannot be extended.",
            ));
        }
        if body.coverage == WarrantyCoverage::None {
            return Err(AppError::validation(
                "Choose how long the warranty should run. To end it, cancel it instead.",
            ));
        }
        let Some(current_end) = warranty.ends_on.clone() else {
            return Err(AppError::conflict(
                codes::WARRANTY_NOT_EXTENDED,
                "This warranty is already for life.",
            ));
        };
        let starts_on = stored_day("starts_on", &warranty.starts_on)?;
        let new_end = match warranty::ends_on(starts_on, body.coverage)? {
            WarrantyEnd::Never => None,
            WarrantyEnd::On(day) => {
                if day <= stored_day("ends_on", &current_end)? {
                    return Err(AppError::conflict(
                        codes::WARRANTY_NOT_EXTENDED,
                        format!(
                            "The new period ends on {day}, which is not later than {current_end}."
                        ),
                    ));
                }
                Some(day.to_string())
            }
        };
        let approver = users::service::require_manager_approval(db, &body.approval).await?;
        let expected = warranty.version;
        warranty.coverage = body.coverage;
        warranty.ends_on = new_end.clone();
        warranty.version += 1;
        warranty.updated_at = BsonDateTime::now();
        if !repository::replace_warranty(db, &warranty, expected).await? {
            return Err(version_conflict());
        }
        let until = new_end.map_or_else(|| "for life".to_string(), |day| format!("to {day}"));
        warranty_events::record(
            db,
            NewEvent {
                warranty_key: &warranty.key,
                claim_key: None,
                kind: WarrantyEventKind::Extended,
                from_status: None,
                to_status: None,
                note: Some(format!(
                    "Extended from {current_end} {until}. {}",
                    approver.reason
                )),
                actor,
                approved_by: Some(&approver),
            },
        )
        .await?;
        detail(db, &warranty.key).await
    })
    .await
}
