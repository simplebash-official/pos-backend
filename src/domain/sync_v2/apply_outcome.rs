use super::derived_scope::DerivedScope;
use super::new_conflict::NewConflict;

/// Result of applying changes for one resource.
#[derive(Debug, Clone, Default)]
pub struct ApplyOutcome {
    pub applied: u32,
    pub duplicates: u32,
    pub conflicts: Vec<NewConflict>,
    pub touched: DerivedScope,
}
