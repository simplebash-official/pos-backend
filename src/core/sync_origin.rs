// Who made the write that is running right now, for cloud sync. Every document
// in a synced collection carries `updated_by_device`; the change-stream
// consumer copies it into `sync_changes.origin_device_id`, and a device's pull
// skips its own echoes by that value. A write that left the previous writer's
// id in place would therefore be hidden from that device forever, so the stamp
// is applied centrally (`clients::tenant_db::ScopedCollection`) instead of
// trusting every repository to remember it.
//
// Nothing here does I/O; the request middleware (`core::middleware::tenant`)
// puts the origin in scope, and `tenant_db` applies the rewrites below.

use std::{future::Future, sync::Arc};

use mongodb::{
    bson::{Bson, Document, doc},
    options::UpdateModifications,
};

/// Field every synced document carries.
pub const ORIGIN_FIELD: &str = "updated_by_device";

/// Origin of a write made outside any device or browser session (background
/// work, internal provisioning). No device ever has this id, so every device
/// receives the change.
pub const CLOUD_ORIGIN: &str = "cloud";

/// Prefix for a browser session's `X-Device-Id`. Keeps a browser from ever
/// sharing an id with a registered device (which would hide that device's
/// own data from it).
pub const WEB_PREFIX: &str = "web:";

/// Mongo collections whose writes are published to the sync change log: the
/// tables of `modules::sync::resources::SYNC_RESOURCES` plus `subcategories`
/// (republished as their parent category). Kept here so `clients` need not
/// depend on `modules`; `tests` in `modules::sync::resources` guard the match.
pub const SYNCED_COLLECTIONS: &[&str] = &[
    "categories",
    "subcategories",
    "suppliers",
    "products",
    "supplier_products",
    "employees",
    "customers",
    "purchases",
    "stock_movements",
    "product_serials",
    "repairs",
    "print_jobs",
    "invoices",
    "payments",
    "credit_notes",
    "users",
];

tokio::task_local! {
    static CURRENT_ORIGIN: Arc<str>;
}

/// Runs `fut` with `origin` as the writer of every synced document it touches.
pub async fn with_origin<F: Future>(origin: impl Into<Arc<str>>, fut: F) -> F::Output {
    CURRENT_ORIGIN.scope(origin.into(), fut).await
}

/// The writer of the running request, if one was put in scope.
pub fn current_origin() -> Option<Arc<str>> {
    CURRENT_ORIGIN.try_with(Clone::clone).ok()
}

/// The origin for a request: a registered device's id, else the browser's
/// `X-Device-Id` (prefixed), else `cloud`.
pub fn origin_for(device_id: Option<&str>, header_device: Option<&str>) -> String {
    if let Some(d) = device_id.map(str::trim).filter(|d| !d.is_empty()) {
        return d.to_string();
    }
    match header_device.map(str::trim).filter(|d| !d.is_empty()) {
        Some(h) => format!("{WEB_PREFIX}{h}"),
        None => CLOUD_ORIGIN.to_string(),
    }
}

pub fn is_synced_collection(name: &str) -> bool {
    SYNCED_COLLECTIONS.contains(&name)
}

/// Sets the origin on a document about to be inserted or used as a
/// replacement.
pub fn stamp_document(origin: &str, mut document: Document) -> Document {
    document.insert(ORIGIN_FIELD, origin);
    document
}

/// Sets the origin on an update. Operator documents get it folded into
/// `$set` (an update with no `$set` gains one); pipeline updates get a final
/// `$set` stage. A replacement-style document (no operators) is stamped like
/// an inserted one.
pub fn stamp_update(origin: &str, update: UpdateModifications) -> UpdateModifications {
    match update {
        UpdateModifications::Document(mut d) => {
            let is_operator_doc = d.keys().next().is_some_and(|k| k.starts_with('$'));
            if !is_operator_doc {
                return UpdateModifications::Document(stamp_document(origin, d));
            }
            // A `$setOnInsert`/`$unset` of the same field would conflict with
            // the `$set` below; the request's origin wins either way.
            for op in ["$setOnInsert", "$unset"] {
                if let Some(Bson::Document(inner)) = d.get_mut(op) {
                    inner.remove(ORIGIN_FIELD);
                }
                if matches!(d.get(op), Some(Bson::Document(inner)) if inner.is_empty()) {
                    d.remove(op);
                }
            }
            match d.get_mut("$set") {
                Some(Bson::Document(set)) => {
                    set.insert(ORIGIN_FIELD, origin);
                }
                _ => {
                    d.insert("$set", doc! { ORIGIN_FIELD: origin });
                }
            }
            UpdateModifications::Document(d)
        }
        UpdateModifications::Pipeline(mut stages) => {
            stages.push(doc! { "$set": { ORIGIN_FIELD: origin } });
            UpdateModifications::Pipeline(stages)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_prefers_the_device_then_the_browser_then_cloud() {
        assert_eq!(origin_for(Some("dev_1"), Some("abc")), "dev_1");
        assert_eq!(origin_for(None, Some("abc")), "web:abc");
        assert_eq!(origin_for(Some(" "), Some(" ")), "cloud");
        assert_eq!(origin_for(None, None), "cloud");
    }

    #[test]
    fn operator_updates_get_the_origin_in_set() {
        let out = stamp_update(
            "web:x",
            UpdateModifications::Document(
                doc! { "$set": { "name": "a", ORIGIN_FIELD: "dev_old" } },
            ),
        );
        let UpdateModifications::Document(d) = out else {
            panic!()
        };
        assert_eq!(
            d.get_document("$set")
                .unwrap()
                .get_str(ORIGIN_FIELD)
                .unwrap(),
            "web:x"
        );
        assert_eq!(
            d.get_document("$set").unwrap().get_str("name").unwrap(),
            "a"
        );
    }

    #[test]
    fn updates_without_set_gain_one_and_conflicts_are_removed() {
        let out = stamp_update(
            "cloud",
            UpdateModifications::Document(doc! {
                "$inc": { "version": 1 },
                "$setOnInsert": { ORIGIN_FIELD: "x" },
            }),
        );
        let UpdateModifications::Document(d) = out else {
            panic!()
        };
        assert_eq!(
            d.get_document("$set")
                .unwrap()
                .get_str(ORIGIN_FIELD)
                .unwrap(),
            "cloud"
        );
        assert!(d.get("$setOnInsert").is_none());
        assert!(d.get("$inc").is_some());
    }

    #[test]
    fn pipeline_updates_get_a_trailing_set_stage() {
        let out = stamp_update(
            "dev_1",
            UpdateModifications::Pipeline(vec![doc! { "$set": { "a": 1 } }]),
        );
        let UpdateModifications::Pipeline(stages) = out else {
            panic!()
        };
        assert_eq!(stages.len(), 2);
        assert_eq!(stages[1], doc! { "$set": { ORIGIN_FIELD: "dev_1" } });
    }
}
