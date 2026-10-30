//! Wire payload -> stored MongoDB document (the inverse of the per-module
//! `hydrate_sync_documents`). The wire DTO is camelCase with RFC3339 strings and
//! hex ids, plus a few read-time enrichments; the stored document is snake_case
//! with typed dates/ObjectIds. Only TOP-LEVEL keys are converted: nested structs
//! (`items`, `returnedItems`, `splitPayments`, ...) are the same Rust types on
//! both sides and serialize identically, so they are copied verbatim.
//!
//! The tables below are derived from the models (`modules/*/model.rs`) and the
//! DTO-vs-document field diff; `tests/sync_cloud_test.rs` round-trips real
//! documents through hydrate -> encode to keep them honest.

mod sync_mongo_codec;
#[cfg(test)]
mod tests;

pub(crate) use sync_mongo_codec::Encoded;
pub(crate) use sync_mongo_codec::encode;
pub(crate) use sync_mongo_codec::encode_generic;

pub(crate) use sync_mongo_codec::plain_derived;
