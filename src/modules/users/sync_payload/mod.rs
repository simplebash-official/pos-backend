//! The wire record for a user in sync (device <-> cloud). Unlike every REST DTO
//! it carries the Argon2id `passwordHash`: a cashier created on one device has to
//! be able to log in on every other device and on the cloud web app, and the
//! hash is the only credential material that can travel. It is meant ONLY for
//! the authenticated sync transport (device tokens on the cloud, the shell's
//! service token on a device); it must never be returned by a REST endpoint,
//! logged, or copied into a conflict record (see `sync::resources::secret_fields`).

mod users_sync_payload;

pub(crate) use users_sync_payload::hydrate_sync_documents;
