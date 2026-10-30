use mongodb::bson::Document;

pub(crate) fn get_i64_flexible(doc: &Document, key: &str) -> i64 {
    doc.get_i64(key)
        .ok()
        .or_else(|| doc.get_i32(key).ok().map(i64::from))
        .or_else(|| doc.get_f64(key).ok().map(|v| v as i64))
        .unwrap_or(0)
}
