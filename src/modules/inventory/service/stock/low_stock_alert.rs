//! The shop notification raised when a product's stock falls to its alert level.

use crate::clients::uow::Unit;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::domain::notifications::{
    NewNotification, NotificationAudience, NotificationCategory, NotificationLink,
    NotificationSeverity, NotificationSource,
};
use crate::modules::inventory::model::ProductDocument;
use crate::modules::notifications;

/// Tells everyone who can see stock that `product` is running low (or ran out), inside the
/// transaction of the movement `movement_key` that made it so.
pub(super) async fn announce(
    unit: impl Into<Unit<'_>>,
    product: &ProductDocument,
    movement_key: &str,
) -> AppResult<()> {
    let left = product.stock_quantity;
    let (severity, title, body) = if left <= 0 {
        (
            NotificationSeverity::Critical,
            format!("Out of stock: {}", product.name),
            format!(
                "{} has run out. Order more before the next sale.",
                product.name
            ),
        )
    } else {
        (
            NotificationSeverity::Warning,
            format!("Low stock: {}", product.name),
            format!(
                "Only {left} left of {} (you asked to be told at {}).",
                product.name, product.min_stock_threshold
            ),
        )
    };
    notifications::service::publish(
        unit,
        NewNotification {
            category: NotificationCategory::Inventory,
            severity,
            title,
            body,
            link: Some(NotificationLink {
                path: "/inventory".to_string(),
                label: "Open stock".to_string(),
            }),
            audience: NotificationAudience::Permission {
                permission: perm::INVENTORY_READ.to_string(),
            },
            source: NotificationSource::Shop,
            dedupe_key: Some(format!("low-stock:{}:{movement_key}", product.key)),
            raised_by_user_key: None,
        },
    )
    .await?;
    Ok(())
}
