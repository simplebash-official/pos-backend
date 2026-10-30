use super::coll::coll;
use super::devices::DEVICES;
use super::min_desktop_app_version::MIN_DESKTOP_APP_VERSION;
use super::parse_app_version::parse_app_version;
use super::update_required::update_required;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::doc;

/// Refuses a device whose registered app is older than
/// [`MIN_DESKTOP_APP_VERSION`], or that never registered a version at all
/// (every app older than the cloud's version check).
pub(crate) async fn ensure_device_app_current(db: &Db, device_id: &str) -> AppResult<()> {
    let version = coll(db, DEVICES)?
        .find_one(doc! { "device_id": device_id })
        .await?
        .and_then(|d| d.get_str("app_version").ok().and_then(parse_app_version));
    match version {
        Some(v) if v >= MIN_DESKTOP_APP_VERSION => Ok(()),
        _ => Err(update_required()),
    }
}
