// Quotations — priced offers an Admin or Manager prints for a customer
// before (and independent of) any sale. A quotation never touches stock,
// payments, invoices or reports; it only stores what was offered and renders
// it as a PDF through document-server. Same routes -> service -> repository
// layering as `modules::print_jobs`, with `repository` private.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
