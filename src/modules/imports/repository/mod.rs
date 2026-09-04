// Mongo-only query/persistence layer for the imports module. Functions
// here never interpret a missing document as an error — they return
// `Option`/`Vec`/counts straight from the driver and leave the "not found"
// -> `AppError` translation to `service`. Visibility is `pub(crate)`.

pub(crate) mod batch;
