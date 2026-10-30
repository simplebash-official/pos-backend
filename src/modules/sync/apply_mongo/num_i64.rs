use mongodb::bson::{Bson, Document};

/// Reads an integer stored as either Int32 or Int64.
pub(crate) fn num_i64(doc: &Document, field: &str) -> Option<i64> {
    match doc.get(field)? {
        Bson::Int64(v) => Some(*v),
        Bson::Int32(v) => Some(i64::from(*v)),
        Bson::Double(v) => Some(*v as i64),
        _ => None,
    }
}
