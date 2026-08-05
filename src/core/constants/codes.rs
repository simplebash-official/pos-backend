//! Standardized status and error code strings used in API response envelopes across all modules.

// System & Auth Status Codes
pub const NOT_FOUND: &str = "NOT_FOUND";
pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const FORBIDDEN: &str = "FORBIDDEN";
pub const ADMIN_REQUIRED: &str = "ADMIN_REQUIRED";
pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";

// Products Status Codes
pub const PRODUCT_NOT_FOUND: &str = "PRODUCT_NOT_FOUND";
pub const SKU_ALREADY_EXISTS: &str = "SKU_ALREADY_EXISTS";
pub const BARCODE_ALREADY_EXISTS: &str = "BARCODE_ALREADY_EXISTS";
pub const INSUFFICIENT_STOCK: &str = "INSUFFICIENT_STOCK";

// Categories & Subcategories Status Codes
pub const CATEGORY_NOT_FOUND: &str = "CATEGORY_NOT_FOUND";
pub const CATEGORY_ALREADY_EXISTS: &str = "CATEGORY_ALREADY_EXISTS";
pub const CATEGORY_HAS_PRODUCTS: &str = "CATEGORY_HAS_PRODUCTS";
pub const CATEGORY_IN_USE: &str = "CATEGORY_IN_USE";
pub const SUBCATEGORY_NOT_FOUND: &str = "SUBCATEGORY_NOT_FOUND";
pub const SUBCATEGORY_ALREADY_EXISTS: &str = "SUBCATEGORY_ALREADY_EXISTS";
pub const SUBCATEGORY_HAS_PRODUCTS: &str = "SUBCATEGORY_HAS_PRODUCTS";
pub const SUBCATEGORY_IN_USE: &str = "SUBCATEGORY_IN_USE";

// Suppliers Status Codes
pub const SUPPLIER_NOT_FOUND: &str = "SUPPLIER_NOT_FOUND";
pub const SUPPLIER_HAS_PURCHASES: &str = "SUPPLIER_HAS_PURCHASES";
pub const SUPPLIER_PRODUCT_LINK_NOT_FOUND: &str = "SUPPLIER_PRODUCT_LINK_NOT_FOUND";

// Users & Auth Status Codes
pub const INVALID_CREDENTIALS: &str = "INVALID_CREDENTIALS";
pub const USER_NOT_FOUND: &str = "USER_NOT_FOUND";
pub const EMAIL_ALREADY_EXISTS: &str = "EMAIL_ALREADY_EXISTS";
pub const USER_INACTIVE: &str = "USER_INACTIVE";
pub const PERMISSION_DENIED: &str = "PERMISSION_DENIED";
pub const ADMIN_ALREADY_EXISTS: &str = "ADMIN_ALREADY_EXISTS";
