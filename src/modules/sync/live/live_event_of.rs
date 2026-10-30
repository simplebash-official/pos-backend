use super::live_event::LiveEvent;

use crate::core::tenancy::TENANT_FIELD;

use mongodb::bson::Document;

pub(super) fn live_event_of(row: &Document) -> Option<(String, LiveEvent)> {
    let tenant = row.get_str(TENANT_FIELD).ok()?.to_string();
    let seq = match row.get("seq")? {
        mongodb::bson::Bson::Int64(n) => *n,
        mongodb::bson::Bson::Int32(n) => i64::from(*n),
        _ => return None,
    };
    Some((
        tenant,
        LiveEvent {
            seq,
            resource: row.get_str("resource").ok()?.to_string(),
            origin: row.get_str("origin_device_id").ok().map(str::to_string),
        },
    ))
}
