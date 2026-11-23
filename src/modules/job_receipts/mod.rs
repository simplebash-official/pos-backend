//! Printing the receipt a customer gets when the shop receives a repair or a print job. Like
//! `documents` it has no routes of its own: `repairs` and `print_jobs` each serve
//! `GET …/{id}/documents/job-receipt` and hand the ticket to this module to print.

pub mod service;
