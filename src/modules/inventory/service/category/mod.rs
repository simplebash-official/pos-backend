// Business rules for category/subcategory management: uniqueness checks
// and the "still referenced by products" guards that block deleting a
// category/subcategory while products still point at it by key. Renaming a
// category's `name` no longer cascades anywhere — products reference a
// category/subcategory by its immutable `key`, not its (renameable) name.

mod add_subcategory;
mod create_category;
mod delete_category;
mod get_category_subcategories;
mod get_valid_categories;
mod hydrate_sync_documents;
mod list_categories;
mod remove_subcategory;
mod update_category;

pub use add_subcategory::add_subcategory;
pub use create_category::create_category;
pub(crate) use delete_category::delete_category;
pub(crate) use get_category_subcategories::get_category_subcategories;
pub(crate) use get_valid_categories::get_valid_categories;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_categories::list_categories;
pub(crate) use remove_subcategory::remove_subcategory;
pub use update_category::update_category;
