// Central source of truth for string/status constants reused across
// modules, so an error code or module tag is never hand-typed (and
// potentially mis-typed) in more than one place.
pub mod codes;
pub mod http_status;
pub mod modules;
pub mod permissions;
pub mod prefixes;
pub mod roles;

