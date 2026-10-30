use super::charge_figure::ChargeFigure;
use super::charge_totals::ChargeTotals;
use super::signed_amount::signed_amount;
use crate::domain::purchase_receipts::BilledBy;

#[must_use]
pub fn charge_totals(charges: &[ChargeFigure]) -> ChargeTotals {
    let mut totals = ChargeTotals::default();
    for charge in charges {
        let amount = signed_amount(charge.kind, charge.amount_cents);
        match charge.billed_by {
            BilledBy::Supplier => totals.supplier_billed_cents += amount,
            BilledBy::ThirdParty => totals.paid_to_others_cents += amount,
        }
    }
    totals
}
