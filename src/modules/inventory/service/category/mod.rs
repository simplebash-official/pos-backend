//! Business rules for category/subcategory management: uniqueness checks
//! and the "still referenced by products" guards that block deleting a
//! category/subcategory while products still point at it by key. Renaming a
//! category's `name` no longer cascades anywhere — products reference a
//! category/subcategory by its immutable `key`, not its (renameable) name.

mod service_category;
mod service_uncategorized;

pub use service_category::add_subcategory;
pub use service_category::create_category;
pub(crate) use service_category::delete_category;
pub(crate) use service_category::get_category_subcategories;
pub(crate) use service_category::get_valid_categories;
pub(crate) use service_category::hydrate_sync_documents;
pub use service_category::list_categories;
pub(crate) use service_category::remove_subcategory;
pub use service_category::update_category;
pub use service_uncategorized::{
    UNCATEGORIZED_CATEGORY_KEY, UNCATEGORIZED_SUBCATEGORY_KEY, ensure_uncategorized,
};
