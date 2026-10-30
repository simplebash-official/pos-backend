/// Rollout phase of a resource. Every resource, `users` included, now syncs in
/// `P3a`; `P3b` is kept for a future resource that must be held back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    P3a,
    P3b,
}
