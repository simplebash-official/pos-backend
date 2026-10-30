//! Mongo access for the `categories` collection only. This collection is
//! the single source of truth for both `GET /categories`-style reads and
//! (together with `repository::subcategory`) product validation
//! (`service::product::ensure_valid_category`) — there is no separate
//! hardcoded category list anywhere else in the codebase.

mod repository_category;

pub(crate) use repository_category::count_categories;
pub(crate) use repository_category::delete_category;
pub(crate) use repository_category::find_category_by_key;
pub(crate) use repository_category::find_category_by_name;
pub(crate) use repository_category::find_category_keys_by_name_pattern;
pub(crate) use repository_category::insert_category;
pub(crate) use repository_category::list_categories_with_subcategories;
pub(crate) use repository_category::update_category_fields;
