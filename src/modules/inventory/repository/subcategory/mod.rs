//! Mongo access for the `subcategories` collection only. Every subcategory
//! belongs to exactly one category (`category_key`), but this collection has
//! no opinion on whether that category still exists — that's a `service`
//! concern (see `service::category`).

mod repository_subcategory;

pub(crate) use repository_subcategory::count_subcategories;
pub(crate) use repository_subcategory::delete_subcategories_by_category;
pub(crate) use repository_subcategory::delete_subcategory_by_key;
pub(crate) use repository_subcategory::find_subcategory_by_category_and_name;
pub(crate) use repository_subcategory::find_subcategory_by_key;
pub(crate) use repository_subcategory::find_subcategory_keys_by_name_pattern;
pub(crate) use repository_subcategory::insert_subcategory;
pub(crate) use repository_subcategory::list_subcategories_by_category;
