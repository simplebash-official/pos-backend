use crate::domain::billing::Invoice;
use crate::domain::employees::Employee;
use crate::domain::inventory::Product;
use crate::domain::print_jobs::PrintJob;
use crate::domain::quotations::Quotation;
use crate::domain::repairs::Repair;

use chrono::{DateTime, Utc};

#[derive(Debug)]
pub struct PulseInput<'a> {
    pub now: DateTime<Utc>,
    pub repairs: &'a [Repair],
    pub print_jobs: &'a [PrintJob],
    pub products: &'a [Product],
    pub employees: &'a [Employee],
    /// Invoices from today (voided ones are ignored here).
    pub today_invoices: &'a [Invoice],
    /// The latest invoices, for the activity stream.
    pub recent_invoices: &'a [Invoice],
    pub active_quotations: &'a [Quotation],
}
