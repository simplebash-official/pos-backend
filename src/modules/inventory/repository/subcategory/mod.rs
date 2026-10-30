// Mongo access for the `subcategories` collection only. Every subcategory
// belongs to exactly one category (`category_key`), but this collection has
// no opinion on whether that category still exists — that's a `service`
// concern (see `service::category`).

mod count_subcategories;
mod delete_subcategories_by_category;
mod delete_subcategory_by_key;
mod find_subcategory_by_category_and_name;
mod find_subcategory_by_key;
mod find_subcategory_keys_by_name_pattern;
mod insert_subcategory;
mod list_subcategories_by_category;
mod row_to_subcategory_doc;
mod subcategories;
mod touch_parent_category;

pub(crate) use count_subcategories::count_subcategories;
pub(crate) use delete_subcategories_by_category::delete_subcategories_by_category;
pub(crate) use delete_subcategory_by_key::delete_subcategory_by_key;
pub(crate) use find_subcategory_by_category_and_name::find_subcategory_by_category_and_name;
pub(crate) use find_subcategory_by_key::find_subcategory_by_key;
pub(crate) use find_subcategory_keys_by_name_pattern::find_subcategory_keys_by_name_pattern;
pub(crate) use insert_subcategory::insert_subcategory;
pub(crate) use list_subcategories_by_category::list_subcategories_by_category;
