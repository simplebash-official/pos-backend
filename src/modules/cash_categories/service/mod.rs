// Business rules for drawer cash categories: who may create what, unique names per
// movement type, similar-name warnings, merging, hiding and the seeded starting list.

mod category_for_expense;
mod category_for_income;
mod category_for_movement;
mod clean_name;
mod clean_name_si;
mod create_category;
mod display_names;
mod ensure_system_categories;
mod is_expense_group;
mod keys_including_merged;
mod list_categories;
mod merge_category;
mod name_max;
mod names_are_similar;
mod other_category_key;
mod other_name;
mod require_category;
mod resolve_categories;
mod resolved_category;
mod similar_categories;
#[cfg(test)]
mod tests;
mod update_category;
mod usage_by_key;
mod usage_of;

pub(crate) use category_for_expense::category_for_expense;
pub(crate) use category_for_income::category_for_income;
pub(crate) use category_for_movement::category_for_movement;
#[cfg(test)]
use clean_name::clean_name;
#[cfg(test)]
use clean_name_si::clean_name_si;
pub use create_category::create_category;
pub(crate) use display_names::display_names;
pub use ensure_system_categories::ensure_system_categories;
pub(crate) use is_expense_group::is_expense_group;
pub(crate) use keys_including_merged::keys_including_merged;
pub use list_categories::list_categories;
pub use merge_category::merge_category;
#[cfg(test)]
use names_are_similar::names_are_similar;
pub use other_category_key::other_category_key;
pub use other_name::OTHER_NAME;
pub(crate) use resolve_categories::resolve_categories;
pub(crate) use resolved_category::ResolvedCategory;
pub use similar_categories::similar_categories;
pub use update_category::update_category;
