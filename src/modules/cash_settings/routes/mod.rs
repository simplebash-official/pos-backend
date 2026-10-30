// HTTP layer for the drawer rules. Anyone who can see cash movements reads them (the cash form needs
// the quick amounts and the float target); only the owner changes them.

mod get_cash_settings;
mod router;
mod update_cash_settings;

pub use router::router;
