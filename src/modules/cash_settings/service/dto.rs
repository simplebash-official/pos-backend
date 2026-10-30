use crate::domain::cash_settings::CashSettings;
use crate::modules::cash_settings::model::CashSettingsDocument;

pub(super) fn dto(d: &CashSettingsDocument) -> CashSettings {
    CashSettings {
        undo_window_seconds: d.undo_window_seconds,
        approval_threshold_cents: d.approval_threshold_cents,
        quick_amounts_cents: d.quick_amounts_cents.clone(),
        float_target_cents: d.float_target_cents,
        version: d.version,
        updated_at: d.updated_at.to_chrono(),
    }
}
