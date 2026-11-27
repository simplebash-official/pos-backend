//! Mongo document shape for the print-jobs feature. Kept separate from
//! `domain::print_jobs` (the API-facing types) so BSON concerns like
//! `ObjectId` never leak into request/response payloads.

mod print_job_document;

pub use print_job_document::PrintJobDocument;
