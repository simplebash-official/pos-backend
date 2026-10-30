//! Pure business types for the inventory feature — no I/O, no Mongo/Axum
//! types beyond serde/utoipa derives. Mongo document shapes live in
//! `modules::inventory::model` and convert into these before a handler wraps
//! them in `core::response::ApiResponse<T>`.

mod domain_inventory_part1;
mod domain_inventory_part2;

pub use domain_inventory_part1::AddSubcategoryRequest;
pub use domain_inventory_part1::BarcodeSource;
pub use domain_inventory_part1::BatchAllocation;
pub use domain_inventory_part1::BatchSource;
pub use domain_inventory_part1::CategoriesResponse;
pub use domain_inventory_part1::CategoryInfo;
pub use domain_inventory_part1::CreateCategoryRequest;
pub use domain_inventory_part1::CreateProductRequest;
pub use domain_inventory_part1::DeleteProductsRequest;
pub use domain_inventory_part1::DeleteProductsResponse;
pub use domain_inventory_part1::InventoryCategoryOverview;
pub use domain_inventory_part1::InventoryMetrics;
pub use domain_inventory_part1::InventoryOverviewQuery;
pub use domain_inventory_part1::InventoryOverviewResponse;
pub use domain_inventory_part1::InventorySubcategoryOverview;
pub use domain_inventory_part1::LowStockItem;
pub use domain_inventory_part1::LowStockResponse;
pub use domain_inventory_part1::OpeningStockRequest;
pub use domain_inventory_part1::PaginationMeta;
pub use domain_inventory_part1::PriceTier;
pub use domain_inventory_part1::Product;
pub use domain_inventory_part1::ProductListQuery;
pub use domain_inventory_part1::ProductListResponse;
pub use domain_inventory_part1::ProductSerial;
pub use domain_inventory_part1::ProductSerialListQuery;
pub use domain_inventory_part1::ProductSerialListResponse;
pub use domain_inventory_part1::ProductSupplierIntake;
pub use domain_inventory_part1::SerialLookup;
pub use domain_inventory_part1::SerialStatus;
pub use domain_inventory_part1::StockAdjustmentRequest;
pub use domain_inventory_part1::StockAdjustmentResponse;
pub use domain_inventory_part1::StockBatch;
pub use domain_inventory_part1::StockBatchListResponse;
pub use domain_inventory_part1::StockMovement;
pub use domain_inventory_part1::StockMovementListQuery;
pub use domain_inventory_part1::StockMovementListResponse;
pub use domain_inventory_part1::StockMovementType;
pub use domain_inventory_part1::StockMovementsResponse;
pub use domain_inventory_part1::SubcategoriesResponse;
pub use domain_inventory_part2::SubcategoryInfo;
pub use domain_inventory_part2::UpdateCategoryRequest;
pub use domain_inventory_part2::UpdateProductRequest;
pub use domain_inventory_part2::UpdateStockBatchRequest;
pub use domain_inventory_part2::ValidCategoriesResponse;
pub use domain_inventory_part2::ValidCategoryOption;
pub use domain_inventory_part2::ValidSubcategoryOption;
