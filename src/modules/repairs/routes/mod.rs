// HTTP layer only — same shape as `modules::customers::routes`. Reads
// require only a valid token; writes additionally require `REPAIRS_WRITE`
// (first route in the codebase to actually enforce that permission — it was
// defined as a placeholder in `core::constants::permissions` until now).

mod create_repair;
mod delete_repair;
mod get_repair;
mod get_repair_stats;
mod list_repairs;
mod router;
mod update_repair;

pub use router::router;
