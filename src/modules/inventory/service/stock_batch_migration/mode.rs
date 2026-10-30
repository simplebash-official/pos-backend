/// Whether a run writes, or only reports what it would write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Apply,
    DryRun,
}
