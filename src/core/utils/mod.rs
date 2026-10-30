//! Small helpers shared across modules: pagination, id parsing, regex escaping and the shop's calendar day.

mod core_utils;

pub use core_utils::SHOP_TZ_OFFSET_SECS;
pub use core_utils::build_bson_regex;
pub use core_utils::calculate_pagination;
pub use core_utils::module_status_response;
pub use core_utils::parse_object_id;
pub use core_utils::regex_escape;
pub use core_utils::shop_day;
pub use core_utils::shop_today_range;
