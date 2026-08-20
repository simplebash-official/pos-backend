use chrono::{DateTime, Duration, Utc};
use mongodb::bson::{oid::ObjectId, raw::CString as BsonCString};

use crate::core::error::{AppError, AppResult};

/// Parses a string into a MongoDB `ObjectId`, returning a `NOT_FOUND` error if invalid.
pub fn parse_object_id(id: &str, entity_name: &str) -> AppResult<ObjectId> {
    ObjectId::parse_str(id).map_err(|_| {
        let code = format!("{}_NOT_FOUND", entity_name.to_uppercase().replace(' ', "_"));
        AppError::not_found_with_code(format!("{entity_name} not found"), code)
    })
}

/// Escapes special regex characters in a search term and strips NUL bytes
/// so it can be safely used in a MongoDB regex query.
pub fn regex_escape(input: &str) -> String {
    let sanitized = input.replace('\0', "");
    let mut escaped = String::with_capacity(sanitized.len());
    for c in sanitized.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Builds a case-insensitive `mongodb::bson::Regex` from a user search string,
/// safely handling regex character escaping and NUL bytes.
pub fn build_bson_regex(search_term: &str) -> mongodb::bson::Regex {
    let escaped = regex_escape(search_term);
    mongodb::bson::Regex {
        pattern: BsonCString::try_from(escaped).expect("NUL bytes were stripped in regex_escape"),
        options: BsonCString::try_from("i").expect("static ASCII options string"),
    }
}

/// Computes pagination parameters `(page, limit, skip)` with default bounds.
pub fn calculate_pagination(
    page: Option<u64>,
    limit: Option<u64>,
    default_limit: u64,
    max_limit: u64,
) -> (u64, u64, u64) {
    let page = page.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(default_limit).clamp(1, max_limit);
    let skip = (page - 1) * limit;
    (page, limit, skip)
}

/// Returns the `[start, end)` UTC bounds of "today", for dashboard "today's
/// X" stats endpoints (billing/repairs/print-jobs). No per-shop timezone is
/// configured anywhere in this app (single-deployment POS), so this
/// intentionally uses UTC-midnight-to-UTC-midnight — the same boundary every
/// `created_at` is already stored against. This can disagree with a
/// browser's *local* "today" near midnight; that's an accepted limitation,
/// not an oversight.
pub fn today_utc_range() -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    let start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always a valid time")
        .and_utc();
    let end = start + Duration::days(1);
    (start, end)
}

/// Creates a standard success response for a feature module's status endpoint.
pub fn module_status_response(
    module_name: &str,
) -> axum::Json<crate::core::response::ApiResponse<crate::domain::ModuleStatusResponse>> {
    axum::Json(crate::core::response::ApiResponse::success(
        crate::domain::ModuleStatusResponse {
            module: module_name.to_string(),
            status: "ok".to_string(),
        },
        format!("{module_name} module status retrieved successfully"),
    ))
}
