//! Renders job receipts through the document-server.

mod job_receipts_service;

pub(crate) use job_receipts_service::load;
pub(crate) use job_receipts_service::print;
