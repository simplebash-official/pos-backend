//! Whose job is on a sale: a repair or print job billed to anyone but its own customer (a walk-in
//! included) must be confirmed on the sale.

use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::billing::CreateSaleItemRequest;
use crate::modules::{print_jobs, repairs};

/// The customer a job on the sale belongs to, as the ticket names them.
#[derive(Debug)]
struct JobOwner {
    ticket_number: String,
    customer_key: String,
    customer_name: String,
}

/// Refuses a sale that bills another customer's job without saying so, before anything is
/// written. `buyer_key` is the sale's registered customer, absent for a walk-in.
///
/// # Errors
///
/// A 409 `TICKET_OTHER_CUSTOMER` when a job's customer is not the buyer and the line does not
/// confirm that customer, a 400 `JOB_CUSTOMER_CONFIRMATION_NOT_ALLOWED` when a line confirms
/// what needs no confirmation, and a 404 for an unknown ticket.
pub(super) async fn check_job_customers(
    db: &Db,
    items: &[CreateSaleItemRequest],
    buyer_key: Option<&str>,
) -> AppResult<()> {
    for item in items {
        let Some(ticket_key) = item.source_ticket_key.as_deref() else {
            continue;
        };
        let owner = match item.source_type.as_str() {
            "repair" => {
                let repair = repairs::service::get_repair(db, ticket_key).await?;
                JobOwner {
                    ticket_number: repair.ticket_number,
                    customer_key: repair.customer_key,
                    customer_name: repair.customer_name,
                }
            }
            "print" => {
                let job = print_jobs::service::get_print_job(db, ticket_key).await?;
                JobOwner {
                    ticket_number: job.ticket_number,
                    customer_key: job.customer_key,
                    customer_name: job.customer_name,
                }
            }
            _ => continue,
        };
        check_confirmation(
            &owner,
            buyer_key,
            item.confirmed_job_customer_key.as_deref(),
        )?;
    }
    Ok(())
}

fn check_confirmation(
    owner: &JobOwner,
    buyer_key: Option<&str>,
    confirmed_key: Option<&str>,
) -> AppResult<()> {
    if buyer_key == Some(owner.customer_key.as_str()) {
        return match confirmed_key {
            None => Ok(()),
            Some(_) => Err(AppError::validation_with_code(
                format!(
                    "{} belongs to the customer on this sale, so there is nothing to confirm.",
                    owner.ticket_number
                ),
                codes::JOB_CUSTOMER_CONFIRMATION_NOT_ALLOWED,
            )),
        };
    }
    if confirmed_key == Some(owner.customer_key.as_str()) {
        return Ok(());
    }
    Err(AppError::conflict(
        codes::TICKET_OTHER_CUSTOMER,
        format!(
            "{} belongs to {}, not the customer on this sale. Confirm it on the bill first.",
            owner.ticket_number, owner.customer_name
        ),
    ))
}
