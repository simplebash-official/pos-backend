//! Printing the slip a customer gets for the repair and print jobs they leave with the shop
//! (`GET /job-receipts/document`). It reads the tickets through `repairs` and `print_jobs` and
//! renders through `documents`; it stores nothing of its own.

pub mod routes;
mod service;
