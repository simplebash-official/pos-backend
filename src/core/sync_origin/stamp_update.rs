use super::origin_field::ORIGIN_FIELD;
use super::stamp_document::stamp_document;
use mongodb::bson::{Bson, doc};
use mongodb::options::UpdateModifications;

/// Sets the origin on an update. Operator documents get it folded into
/// `$set` (an update with no `$set` gains one); pipeline updates get a final
/// `$set` stage. A replacement-style document (no operators) is stamped like
/// an inserted one.
#[must_use]
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
