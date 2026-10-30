use crate::domain::cash_categories::ReportGroup;

/// What a category key stands for today: its live name and report group, following merges.
#[derive(Debug)]
pub(crate) struct ResolvedCategory {
    pub name: String,
    pub report_group: ReportGroup,
}
