// Mongo access for the `categories` collection only. This collection is
// the single source of truth for both `GET /categories`-style reads and
// (together with `repository::subcategory`) product validation
// (`service::product::ensure_valid_category`) — there is no separate
// hardcoded category list anywhere else in the codebase.

mod categories;
mod count_categories;
mod delete_category;
mod find_category_by_key;
mod find_category_by_name;
mod find_category_keys_by_name_pattern;
mod insert_category;
mod list_categories_with_subcategories;
mod row_to_category_doc;
mod update_category_fields;

pub(crate) use count_categories::count_categories;
pub(crate) use delete_category::delete_category;
pub(crate) use find_category_by_key::find_category_by_key;
pub(crate) use find_category_by_name::find_category_by_name;
pub(crate) use find_category_keys_by_name_pattern::find_category_keys_by_name_pattern;
pub(crate) use insert_category::insert_category;
pub(crate) use list_categories_with_subcategories::list_categories_with_subcategories;
pub(crate) use update_category_fields::update_category_fields;
