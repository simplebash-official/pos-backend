// Index bootstrap, run once at startup.
//
// Two of these are correctness-critical, not just performance:
//
//   * `(updated_at, key)` is the exact sort `modules::sync::repository`
//     pages by. Without it every `/sync/changes` call is a collection scan,
//     and MongoDB aborts an unindexed in-memory sort above 32 MB — so sync
//     would start failing outright once a shop's history grows.
//   * The unique `(key, user_id)` on `idempotency_keys` is what makes the
//     duplicate-insert branch in `core::middleware::idempotency` reachable
//     at all. Without it `insert_one` always succeeds, both racing requests
//     execute, and the replay guard silently does nothing.
//
// Creating an index that already exists with the same spec is a no-op, so
// this is safe to run on every boot.
//
// The set of indexes is built as plain data (`index_specs`) so it can be unit
// tested. With `multi_tenant` every index leads with `tenant_id` (unique ones
// become unique *per tenant*) and gets a `t_`-prefixed name so it cannot clash
// with a same-named single-tenant index. The idempotency TTL index is the one
// exception: a TTL index must be single-field.

mod ensure_indexes;
mod index_spec;
mod index_specs;
#[cfg(test)]
mod tests;

pub use ensure_indexes::ensure_indexes;
pub use index_spec::IndexSpec;
pub use index_specs::index_specs;
