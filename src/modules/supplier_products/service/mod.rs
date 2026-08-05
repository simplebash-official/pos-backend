// Business rules and orchestration for the supplier-product linking
// module. Calls into `super::repository` for all Mongo access and returns
// domain types (never BSON/`ObjectId`) to `routes`.

pub mod link;
