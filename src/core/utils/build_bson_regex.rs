use super::regex_escape::regex_escape;

use mongodb::bson::raw::CString as BsonCString;

/// Builds a case-insensitive `mongodb::bson::Regex` from a user search string,
/// safely handling regex character escaping and NUL bytes.
#[must_use]
pub fn build_bson_regex(search_term: &str) -> mongodb::bson::Regex {
    let escaped = regex_escape(search_term);
    mongodb::bson::Regex {
        pattern: BsonCString::try_from(escaped).expect("NUL bytes were stripped in regex_escape"),
        options: BsonCString::try_from("i").expect("static ASCII options string"),
    }
}
