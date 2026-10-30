/// What a run did (or, in a dry run, would do).
#[derive(Debug, Default, Clone, Copy)]
pub struct Report {
    pub shops_checked: usize,
    pub movements: usize,
}
