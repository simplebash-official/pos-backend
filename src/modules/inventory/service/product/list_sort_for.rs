use super::sort_field_for::sort_field_for;

use mongodb::bson::Document;

/// Sort document for `GET /products`. With no `sortBy` the list is ordered by
/// name A-Z so it stays put when products are edited or sold (the old default,
/// `updated_at DESC`, made rows jump around). An explicit `sortBy` keeps the
/// historical "desc unless `sortOrder=asc`" rule. Ties always fall back to
/// name then `_id`, so equal values never swap between requests.
pub(crate) fn list_sort_for(sort_by: Option<&str>, sort_order: Option<&str>) -> Document {
    let (field, ascending) = match sort_by {
        None => ("name", sort_order != Some("desc")),
        Some(_) => (sort_field_for(sort_by), sort_order == Some("asc")),
    };
    let mut sort = Document::new();
    sort.insert(field, if ascending { 1 } else { -1 });
    if field != "name" {
        sort.insert("name", 1);
    }
    sort.insert("_id", 1);
    sort
}
