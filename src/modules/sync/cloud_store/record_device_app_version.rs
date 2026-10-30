use super::coll::coll;
use super::devices::DEVICES;
use super::min_desktop_app_version::MIN_DESKTOP_APP_VERSION;
use super::parse_app_version::parse_app_version;
use super::update_required::update_required;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

/// Records the app version a device registered with, refusing one too old
/// to sync (see [`MIN_DESKTOP_APP_VERSION`]).
pub(crate) async fn record_device_app_version(
    db: &Db,
    device_id: &str,
    app_version: &str,
) -> AppResult<()> {
    if parse_app_version(app_version).is_none_or(|v| v < MIN_DESKTOP_APP_VERSION) {
        return Err(update_required());
    }
    coll(db, DEVICES)?
        .update_one(
            doc! { "device_id": device_id },
            doc! { "$set": { "app_version": app_version } },
        )
        .await?;
    Ok(())
}
