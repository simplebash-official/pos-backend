// Business rules and orchestration for the inventory module: validation,
// cross-collection sequencing, and translating "missing" repository results
// into the right `AppError`. Calls into `super::repository` for all Mongo
// access and returns domain types (never BSON/`ObjectId`) to `routes`.

pub mod category;
pub mod overview;
pub mod product;
pub mod sku;
pub mod stats;
pub mod stock;
