// Business rules and orchestration for the inventory module: validation,
// cross-collection sequencing, and translating "missing" repository results
// into the right `AppError`. Calls into `super::repository` for all Mongo
// access and returns domain types (never BSON/`ObjectId`) to `routes`.

pub(crate) mod category;
pub(crate) mod product;
pub(crate) mod stock;
