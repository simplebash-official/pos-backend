//! Pure business types for the inventory feature — no I/O, no Mongo/Axum
//! types beyond serde/utoipa derives. Mongo document shapes live in
//! `modules::inventory::model` and convert into these before a handler wraps
//! them in `core::response::ApiResponse<T>`.

mod category;
mod overview;
mod product;
mod serial;
mod stock;

pub use category::AddSubcategoryRequest;
pub use category::CategoriesResponse;
pub use category::CategoryInfo;
pub use category::CreateCategoryRequest;
pub use category::SubcategoriesResponse;
pub use category::SubcategoryInfo;
pub use category::UpdateCategoryRequest;
pub use category::ValidCategoriesResponse;
pub use category::ValidCategoryOption;
pub use category::ValidSubcategoryOption;
pub use overview::InventoryCategoryOverview;
pub use overview::InventoryMetrics;
pub use overview::InventoryOverviewQuery;
pub use overview::InventoryOverviewResponse;
pub use overview::InventorySubcategoryOverview;
pub use product::BarcodeSource;
pub use product::CreateProductRequest;
pub use product::DeleteProductsRequest;
pub use product::DeleteProductsResponse;
pub use product::OpeningStockRequest;
pub use product::PaginationMeta;
pub use product::PriceTier;
pub use product::Product;
pub use product::ProductListQuery;
pub use product::ProductListResponse;
pub use product::ProductSupplierIntake;
pub use product::UpdateProductRequest;
pub use serial::ProductSerial;
pub use serial::ProductSerialListQuery;
pub use serial::ProductSerialListResponse;
pub use serial::SerialLookup;
pub use serial::SerialStatus;
pub use stock::BatchAllocation;
pub use stock::BatchSource;
pub use stock::LowStockItem;
pub use stock::LowStockResponse;
pub use stock::StockAdjustmentRequest;
pub use stock::StockAdjustmentResponse;
pub use stock::StockBatch;
pub use stock::StockBatchListResponse;
pub use stock::StockMovement;
pub use stock::StockMovementListQuery;
pub use stock::StockMovementListResponse;
pub use stock::StockMovementType;
pub use stock::StockMovementsResponse;
pub use stock::UpdateStockBatchRequest;
