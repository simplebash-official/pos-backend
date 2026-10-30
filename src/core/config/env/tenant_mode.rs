/// Whether one deployment serves a single shop or many isolated tenants.
/// `Multi` requires MongoDB and makes every database handle tenant-scoped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantMode {
    Single,
    Multi,
}
