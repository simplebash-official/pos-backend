// Business rules for the drawer rules: seeding the row with the shipped starting values, validating
// edits, and reading the rules for the cash movement code. A missing row is an error, never a default.

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::prefixes::CASH_SETTINGS_KEY,
        error::{AppError, AppResult},
        logging::domain::tracked,
    },
    domain::cash_settings::{
        CashSettings, MAX_QUICK_AMOUNTS, MAX_RULE_CENTS, STARTING_QUICK_AMOUNTS_CENTS,
        STARTING_UNDO_WINDOW_SECONDS, UNDO_WINDOW_RANGE, UpdateCashSettingsRequest,
    },
    modules::cash_settings::{model::CashSettingsDocument, repository},
};

fn dto(d: &CashSettingsDocument) -> CashSettings {
    CashSettings {
        undo_window_seconds: d.undo_window_seconds,
        approval_threshold_cents: d.approval_threshold_cents,
        quick_amounts_cents: d.quick_amounts_cents.clone(),
        float_target_cents: d.float_target_cents,
        version: d.version,
        updated_at: d.updated_at.to_chrono(),
    }
}

/// Creates the shop's drawer rules with the shipped starting values when the row is missing. Never
/// touches an existing row, so the owner's edits survive a re-run.
pub async fn ensure_settings(db: &Db) -> AppResult<()> {
    if repository::find(db, CASH_SETTINGS_KEY).await?.is_some() {
        return Ok(());
    }
    // A fixed time, like the built-in categories, so the same row made on two devices is one row for sync.
    let seeded_at = BsonDateTime::from_millis(1_767_225_600_000);
    repository::insert(
        db,
        CashSettingsDocument {
            id: None,
            key: CASH_SETTINGS_KEY.to_string(),
            undo_window_seconds: STARTING_UNDO_WINDOW_SECONDS,
            approval_threshold_cents: None,
            quick_amounts_cents: STARTING_QUICK_AMOUNTS_CENTS.to_vec(),
            float_target_cents: None,
            updated_by_name_snapshot: None,
            version: 1,
            created_at: seeded_at,
            updated_at: seeded_at,
            updated_by_device: None,
        },
    )
    .await?;
    Ok(())
}

async fn document(db: &Db) -> AppResult<CashSettingsDocument> {
    repository::find(db, CASH_SETTINGS_KEY).await?.ok_or_else(|| {
        AppError::internal("The drawer rules have not been set up for this shop".to_string())
    })
}

/// The shop's drawer rules.
pub async fn get_settings(db: &Db) -> AppResult<CashSettings> {
    Ok(dto(&document(db).await?))
}

/// Seconds a cashier can undo their own movement.
pub(crate) async fn undo_window_seconds(db: &Db) -> AppResult<i64> {
    Ok(document(db).await?.undo_window_seconds)
}

/// The shop-wide payout limit that needs a manager PIN, when one is set.
pub(crate) async fn approval_threshold_cents(db: &Db) -> AppResult<Option<i64>> {
    Ok(document(db).await?.approval_threshold_cents)
}

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
fn validate(body: &UpdateCashSettingsRequest) -> AppResult<()> {
    let (low, high) = UNDO_WINDOW_RANGE;
    if !(low..=high).contains(&body.undo_window_seconds) {
        return Err(AppError::validation(format!("The undo window must be between {low} and {high} seconds.")));
    }
    if let Some(limit) = body.approval_threshold_cents {
        check_money("The approval limit", limit)?;
    }
    if let Some(target) = body.float_target_cents {
        check_money("The float target", target)?;
    }
    let chips = &body.quick_amounts_cents;
    if chips.is_empty() || chips.len() > MAX_QUICK_AMOUNTS {
        return Err(AppError::validation(format!("Please give between 1 and {MAX_QUICK_AMOUNTS} quick amounts.")));
    }
    for cents in chips {
        check_money("A quick amount", *cents)?;
    }
    if chips.windows(2).any(|w| w[0] >= w[1]) {
        return Err(AppError::validation("Quick amounts must go from smallest to largest, with no repeats."));
    }
    Ok(())
}

/// Replaces the drawer rules after checking them.
pub async fn update_settings(db: &Db, set_by_name: String, body: UpdateCashSettingsRequest) -> AppResult<CashSettings> {
    tracked("cash_settings.updated", async move {
        validate(&body)?;
        let mut doc = document(db).await?;
        doc.undo_window_seconds = body.undo_window_seconds;
        doc.approval_threshold_cents = body.approval_threshold_cents;
        doc.quick_amounts_cents = body.quick_amounts_cents;
        doc.float_target_cents = body.float_target_cents;
        doc.updated_by_name_snapshot = Some(set_by_name);
        doc.version += 1;
        doc.updated_at = BsonDateTime::now();
        repository::save(db, &doc).await?;
        Ok(dto(&doc))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> UpdateCashSettingsRequest {
        UpdateCashSettingsRequest {
            undo_window_seconds: 120,
            approval_threshold_cents: Some(50_000),
            quick_amounts_cents: vec![10_000, 50_000],
            float_target_cents: None,
        }
    }

    #[test]
    fn a_good_set_of_rules_passes() {
        assert!(validate(&ok()).is_ok());
    }

    #[test]
    fn each_rule_is_checked() {
        let bad = |f: fn(&mut UpdateCashSettingsRequest)| {
            let mut b = ok();
            f(&mut b);
            validate(&b).is_err()
        };
        assert!(bad(|b| b.undo_window_seconds = 5));
        assert!(bad(|b| b.undo_window_seconds = 4000));
        assert!(bad(|b| b.approval_threshold_cents = Some(0)));
        assert!(bad(|b| b.float_target_cents = Some(-1)));
        assert!(bad(|b| b.quick_amounts_cents = vec![]));
        assert!(bad(|b| b.quick_amounts_cents = vec![5_000, 5_000]));
        assert!(bad(|b| b.quick_amounts_cents = vec![50_000, 10_000]));
        assert!(bad(|b| b.quick_amounts_cents = (1..=9).map(|n| n * 1000).collect()));
    }
}
