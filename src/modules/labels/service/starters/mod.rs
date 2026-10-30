// The ready-made label designs every shop starts with. They live in code, not in
// the database, so they are identical for every shop, always present and can
// never be edited or deleted; a shop copies one to make its own.

mod all;
mod find;
mod starter_key_prefix;
#[cfg(test)]
mod tests;

pub(crate) use all::all;
pub(crate) use find::find;
pub(crate) use starter_key_prefix::STARTER_KEY_PREFIX;
