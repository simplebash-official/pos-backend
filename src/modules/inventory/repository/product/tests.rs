use super::build_product_where_clause::build_product_where_clause;
use super::like_pattern::like_pattern;
use crate::clients::sqlite::SqlFilter;

use mongodb::bson::doc;

use crate::core::utils::build_bson_regex;

#[test]
fn a_search_becomes_an_escaped_like_pattern() {
    let clause = |term: &str| doc! { "$regex": build_bson_regex(term) };
    assert_eq!(like_pattern(&clause("Mug")).as_deref(), Some("%Mug%"));
    // Regex escaping is undone; LIKE's own wildcards are escaped.
    assert_eq!(
        like_pattern(&clause("2.5-inch (Black)")).as_deref(),
        Some("%2.5-inch (Black)%")
    );
    assert_eq!(
        like_pattern(&clause("50%_off")).as_deref(),
        Some("%50\\%\\_off%")
    );
    assert_eq!(like_pattern(&clause("a\\b")).as_deref(), Some("%a\\\\b%"));
    assert_eq!(
        like_pattern(&doc! { "$regex": "plain" }).as_deref(),
        Some("%plain%")
    );
    assert_eq!(like_pattern(&doc! { "$regex": 5 }), None);
}

#[test]
fn a_search_filters_name_sku_and_barcode_on_sqlite() {
    let pattern = build_bson_regex("Mug");
    let filter = doc! { "$and": [ { "$or": [
        { "name": { "$regex": pattern.clone() } },
        { "sku": { "$regex": pattern.clone() } },
        { "barcode": { "$regex": pattern } },
    ] } ] };
    let SqlFilter { clause, bindings } = build_product_where_clause(&filter);
    assert!(
            clause.contains("(p.name LIKE ? ESCAPE '\\' OR p.sku LIKE ? ESCAPE '\\' OR p.barcode LIKE ? ESCAPE '\\')"),
            "{clause}"
        );
    assert_eq!(bindings, ["%Mug%", "%Mug%", "%Mug%"]);
}
