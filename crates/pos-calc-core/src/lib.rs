//! Server-side calculations for the point of sale: checkout, receipts, quotations, earnings, shares, cash counts and warranty periods. All money is integer cents. The backend serves these over HTTP and the browser runs the same code as WebAssembly, so a screen and the server can never disagree.

pub mod calculations;
pub mod cash;
pub mod checkout;
pub mod earnings;
pub mod error;
pub mod quotation;
pub mod receipt;
pub mod shares;
pub mod warranty;

pub use error::{CalcError, CalcResult};
