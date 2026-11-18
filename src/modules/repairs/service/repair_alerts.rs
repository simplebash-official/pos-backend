//! The shop notifications a repair job raises when it changes hands: ready for pickup, and
//! handed back.

use crate::clients::db::Db;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::domain::notifications::{
    NewNotification, NotificationAudience, NotificationCategory, NotificationLink,
    NotificationSeverity, NotificationSource,
};
use crate::domain::repairs::Repair;
use crate::modules::notifications;

/// Tells everyone who works on repairs that `repair` just became ready or was handed back.
/// Any other status change, or a write that kept the status, raises nothing.
pub(super) async fn announce_status_change(
    db: &Db,
    repair: &Repair,
    previous_status: &str,
) -> AppResult<()> {
    if repair.status == previous_status {
        return Ok(());
    }
    let (severity, title, body) = match repair.status.as_str() {
        "ready" => (
            NotificationSeverity::Success,
            "Repair ready for pickup",
            format!(
                "{} for {} ({}) is ready. Let the customer know.",
                repair.device_model, repair.customer_name, repair.ticket_number
            ),
        ),
        "delivered" => (
            NotificationSeverity::Info,
            "Repair handed back",
            format!(
                "{} ({}) was handed back to {}.",
                repair.device_model, repair.ticket_number, repair.customer_name
            ),
        ),
        _ => return Ok(()),
    };
    notifications::service::publish(
        db,
        NewNotification {
            category: NotificationCategory::Repairs,
            severity,
            title: title.to_string(),
            body,
            link: Some(NotificationLink {
                path: format!("/repairs?jobKey={}", repair.key),
                label: "Open job".to_string(),
            }),
            audience: NotificationAudience::Permission {
                permission: perm::REPAIRS_WRITE.to_string(),
            },
            source: NotificationSource::Shop,
            dedupe_key: Some(format!("repair:{}:{}", repair.key, repair.status)),
            raised_by_user_key: None,
        },
    )
    .await?;
    Ok(())
}
