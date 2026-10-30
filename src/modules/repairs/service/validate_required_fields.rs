use crate::core::error::{AppError, AppResult};

fn validate_phone_number(phone: &str, field_name: &str) -> AppResult<()> {
    let trimmed = phone.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(format!(
            "{field_name} cannot be empty"
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(format!(
            "{field_name} may only contain digits, spaces, dashes, and a plus sign"
        )));
    }
    Ok(())
}

pub(super) fn validate_required_fields(
    customer_name: &str,
    customer_phone: &str,
    device_model: &str,
    issue_description: &str,
    estimated_cost_cents: Option<i64>,
) -> AppResult<()> {
    if customer_name.trim().is_empty() {
        return Err(AppError::validation("Customer name is required"));
    }
    validate_phone_number(customer_phone, "Customer phone")?;
    if device_model.trim().is_empty() {
        return Err(AppError::validation("Device model is required"));
    }
    if issue_description.trim().is_empty() {
        return Err(AppError::validation("Issue description is required"));
    }
    if let Some(cost) = estimated_cost_cents
        && cost < 0
    {
        return Err(AppError::validation("Estimated cost cannot be negative"));
    }
    Ok(())
}
