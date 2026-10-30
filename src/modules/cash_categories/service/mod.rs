//! Business rules for drawer cash categories: who may create what, unique names per
//! movement type, similar-name warnings, merging, hiding and the seeded starting list.

mod cash_categories_service;
#[cfg(test)]
mod tests;

pub use cash_categories_service::OTHER_NAME;
pub(crate) use cash_categories_service::ResolvedCategory;
pub(crate) use cash_categories_service::category_for_expense;
pub(crate) use cash_categories_service::category_for_income;
pub(crate) use cash_categories_service::category_for_movement;
#[cfg(test)]
use cash_categories_service::clean_name;
#[cfg(test)]
use cash_categories_service::clean_name_si;
pub use cash_categories_service::create_category;
pub(crate) use cash_categories_service::display_names;
pub use cash_categories_service::ensure_system_categories;
pub(crate) use cash_categories_service::is_expense_group;
pub(crate) use cash_categories_service::keys_including_merged;
pub use cash_categories_service::list_categories;
pub use cash_categories_service::merge_category;
#[cfg(test)]
use cash_categories_service::names_are_similar;
pub use cash_categories_service::other_category_key;
pub(crate) use cash_categories_service::resolve_categories;
pub use cash_categories_service::similar_categories;
pub use cash_categories_service::update_category;
