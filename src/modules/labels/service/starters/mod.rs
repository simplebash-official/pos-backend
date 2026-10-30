//! The ready-made label designs every shop starts with. They live in code, not in
//! the database, so they are identical for every shop, always present and can
//! never be edited or deleted; a shop copies one to make its own.

mod service_starters;
#[cfg(test)]
mod tests;

pub(crate) use service_starters::STARTER_KEY_PREFIX;
pub(crate) use service_starters::all;
pub(crate) use service_starters::find;
