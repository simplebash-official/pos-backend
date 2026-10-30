//! HTTP layer for the drawer rules. Anyone who can see cash movements reads them (the cash form needs
//! the quick amounts and the float target); only the owner changes them.

mod cash_settings_routes;

pub use cash_settings_routes::router;
