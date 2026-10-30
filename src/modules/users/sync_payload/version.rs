use mongodb::bson::{Bson, Document};

pub(super) fn version(doc: &Document) -> i64 {
    match doc.get("version") {
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Int32(v)) => i64::from(*v),
        _ => 1,
    }
}
