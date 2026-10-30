use crate::core::error::{AppError, AppResult};
use crate::domain::cash_settings::{
    MAX_QUICK_AMOUNTS, MAX_RULE_CENTS, UNDO_WINDOW_RANGE, UpdateCashSettingsRequest,
};

fn check_money(label: &str, cents: i64) -> AppResult<()> {
    if cents <= 0 || cents > MAX_RULE_CENTS {
        return Err(AppError::validation(format!(
            "{label} must be more than zero and at most Rs. {}.",
            MAX_RULE_CENTS / 100
        )));
    }
    Ok(())
}

/// Checks a whole set of rules; the first problem is returned in plain words.
pub(super) fn validate(body: &UpdateCashSettingsRequest) -> AppResult<()> {
    let (low, high) = UNDO_WINDOW_RANGE;
    if !(low..=high).contains(&body.undo_window_seconds) {
        return Err(AppError::validation(format!(
            "The undo window must be between {low} and {high} seconds."
        )));
    }
    if let Some(limit) = body.approval_threshold_cents {
        check_money("The approval limit", limit)?;
    }
    if let Some(target) = body.float_target_cents {
        check_money("The float target", target)?;
    }
    let chips = &body.quick_amounts_cents;
    if chips.is_empty() || chips.len() > MAX_QUICK_AMOUNTS {
        return Err(AppError::validation(format!(
            "Please give between 1 and {MAX_QUICK_AMOUNTS} quick amounts."
        )));
    }
    for cents in chips {
        check_money("A quick amount", *cents)?;
    }
    if chips.windows(2).any(|w| w[0] >= w[1]) {
        return Err(AppError::validation(
            "Quick amounts must go from smallest to largest, with no repeats.",
        ));
    }
    Ok(())
}
