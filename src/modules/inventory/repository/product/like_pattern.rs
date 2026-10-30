use mongodb::bson::{Bson, Document};

/// The SQLite `LIKE` pattern (`%text%`, with `%`, `_` and `\` escaped for
/// `ESCAPE '\'`) for a `{ "$regex": … }` search clause. `service::product`
/// sends a BSON regex built by `build_bson_regex`, whose text is regex-escaped,
/// so the escaping is undone first; a plain string is taken as it is.
pub(super) fn like_pattern(clause: &Document) -> Option<String> {
    let regex = match clause.get("$regex")? {
        Bson::RegularExpression(regex) => regex.pattern.to_string(),
        Bson::String(text) => text.clone(),
        _ => return None,
    };
    let mut text = String::with_capacity(regex.len() + 2);
    text.push('%');
    let mut chars = regex.chars();
    while let Some(c) = chars.next() {
        // `regex_escape` puts a backslash before each special character.
        let literal = match c {
            '\\' => chars.next().unwrap_or('\\'),
            other => other,
        };
        if matches!(literal, '%' | '_' | '\\') {
            text.push('\\');
        }
        text.push(literal);
    }
    text.push('%');
    Some(text)
}
