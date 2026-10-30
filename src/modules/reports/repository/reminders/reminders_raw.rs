use crate::domain::reports::ReminderEntry;

/// The full reminder set (already merged + sorted, but NOT truncated) plus
/// honest headline counts over that full set.
#[derive(Debug)]
pub(crate) struct RemindersRaw {
    pub entries: Vec<ReminderEntry>,
    pub credit_overdue_count: u64,
    pub credit_overdue_amount_cents: i64,
    pub credit_due_soon_count: u64,
    pub credit_due_soon_amount_cents: i64,
    pub jobs_overdue_count: u64,
    pub jobs_due_soon_count: u64,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
