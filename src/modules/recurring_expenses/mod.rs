// Regular bills: templates for what the shop pays every month, the reminders worked out from them,
// and the "already paid" check against what was actually recorded. The costs themselves live in the
// expense ledger and the cash drawer; this module only stores the templates.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
