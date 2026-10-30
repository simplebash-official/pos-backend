/// Tables excluded from backups by design (ephemeral locks, query caches, migrations metadata).
const EXCLUDED_TABLES: &[&str] = &["idempotency_keys", "_sqlx_migrations"];

/// A table that must not travel in a backup: the fixed exclusions plus every
/// `sync_*` bookkeeping table. Restoring another install's sync state (device
/// id, outbox, merge metadata) would make two devices share an identity.
pub(super) fn is_excluded(name: &str) -> bool {
    EXCLUDED_TABLES.contains(&name) || name.starts_with("sync_")
}
