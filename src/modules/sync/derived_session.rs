//! Pure ledger accumulation rules shared between cloud (`derived_mongo`) and
//! device (`derived_sqlite`) recomputation for `register_sessions` and `drawer_state`.

/// Prefix of payment notes emitted by `void_credit_note`; these reversal rows
/// must not count toward session sales totals.
const CREDIT_NOTE_REVERSAL_PREFIX: &str = "Reversal of credit note ";

/// Prefix of payment notes emitted when an exchange credit note collects extra
/// payment from the customer; `create_credit_note` does not bump session sales.
const EXCHANGE_DIFFERENCE_PREFIX: &str = "Payment for exchange/credit note difference ";

/// Ledger-derived counters and state for one `register_sessions` row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionLedger {
    pub total_cash_sales_cents: i64,
    pub total_card_sales_cents: i64,
    pub total_online_sales_cents: i64,
    pub total_credit_sales_cents: i64,
    pub total_cash_refunds_cents: i64,
    pub total_pay_in_cents: i64,
    pub total_pay_out_cents: i64,
    pub total_owner_withdrawals_cents: i64,
    pub total_cash_drops_cents: i64,
    pub has_late_movements: bool,
}

impl SessionLedger {
    /// Accumulates one non-deleted payment linked to the session.
    pub(crate) fn record_payment(
        &mut self,
        amount_cents: i64,
        payment_method: &str,
        notes: Option<&str>,
    ) {
        if amount_cents > 0 {
            let text = notes.unwrap_or("");
            if text.starts_with(CREDIT_NOTE_REVERSAL_PREFIX)
                || text.starts_with(EXCHANGE_DIFFERENCE_PREFIX)
            {
                return;
            }
            match payment_method {
                "cash" => self.total_cash_sales_cents += amount_cents,
                "card" => self.total_card_sales_cents += amount_cents,
                "online" => self.total_online_sales_cents += amount_cents,
                _ => {}
            }
        } else if amount_cents < 0 && payment_method == "cash" {
            self.total_cash_refunds_cents += -amount_cents;
        }
    }

    /// Accumulates the unpaid balance booked at checkout on one credit invoice
    /// linked to the session.
    pub(crate) fn record_credit_invoice(
        &mut self,
        total_cents: i64,
        amount_received_cents: Option<i64>,
    ) {
        let unpaid = (total_cents - amount_received_cents.unwrap_or(0)).max(0);
        self.total_credit_sales_cents += unpaid;
    }

    /// Accumulates one non-deleted cash movement linked to the session.
    pub(crate) fn record_movement(
        &mut self,
        movement_type: &str,
        amount_cents: i64,
        reversed: bool,
        recorded_late: bool,
    ) {
        let signed = if reversed {
            -amount_cents
        } else {
            amount_cents
        };
        match movement_type {
            "add_cash" => self.total_pay_in_cents += signed,
            "petty_expense" | "rent_and_bills" | "staff_payment" | "supplier_payment" => {
                self.total_pay_out_cents += signed;
            }
            "owner_withdrawal" => self.total_owner_withdrawals_cents += signed,
            "cash_drop" => self.total_cash_drops_cents += signed,
            _ => {}
        }
        if recorded_late {
            self.has_late_movements = true;
        }
    }

    /// Computes expected drawer cash from `opening_float_cents` and the session ledger.
    #[must_use]
    pub(crate) fn expected_cash(&self, opening_float_cents: i64) -> i64 {
        opening_float_cents + self.total_cash_sales_cents + self.total_pay_in_cents
            - self.total_pay_out_cents
            - self.total_cash_refunds_cents
            - self.total_owner_withdrawals_cents
            - self.total_cash_drops_cents
    }

    /// Returns `(expected_cash_cents, cash_variance_cents)` matching the session status.
    /// Open and reopened shifts leave both `None`; closing/closed shifts persist both.
    #[must_use]
    pub(crate) fn closing_cash(
        &self,
        status: &str,
        opening_float_cents: i64,
        counted_cash_cents: Option<i64>,
    ) -> (Option<i64>, Option<i64>) {
        if matches!(status, "closing" | "closed") {
            let expected = self.expected_cash(opening_float_cents);
            let variance = counted_cash_cents.map(|counted| counted - expected);
            (Some(expected), variance)
        } else {
            (None, None)
        }
    }
}

/// Drawer balance delta contributed by one cash movement (negated when `reversed`).
#[must_use]
pub(crate) fn drawer_delta_for_movement(
    movement_type: &str,
    amount_cents: i64,
    reversed: bool,
) -> i64 {
    let base = match movement_type {
        "add_cash" => amount_cents,
        "petty_expense" | "rent_and_bills" | "staff_payment" | "supplier_payment"
        | "owner_withdrawal" | "cash_drop" => -amount_cents,
        _ => 0,
    };
    if reversed { -base } else { base }
}

/// Parses an RFC 3339 timestamp string into epoch milliseconds (0 if malformed).
#[must_use]
pub(crate) fn parse_rfc3339_ms(raw: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::SessionLedger;
    use super::drawer_delta_for_movement;
    use super::parse_rfc3339_ms;

    #[test]
    fn ledger_aggregates_sales_refunds_credit_movements_and_reversals() {
        let mut ledger = SessionLedger::default();
        ledger.record_payment(5_000, "cash", None);
        ledger.record_payment(3_000, "card", Some("Card ending 4242"));
        ledger.record_payment(2_000, "online", None);
        // Reversal of a voided credit note must not increment cash sales.
        ledger.record_payment(
            1_200,
            "cash",
            Some("Reversal of credit note CN-000001 (voided)"),
        );
        // Partial credit sale deposit (paid cash) + unpaid credit balance.
        ledger.record_payment(1_500, "cash", Some("Deposit at checkout"));
        ledger.record_credit_invoice(6_000, Some(1_500));
        // Cash refund reduces cash expectation.
        ledger.record_payment(-800, "cash", Some("Refund for credit note CN-000002"));
        ledger.record_payment(-400, "card", Some("Refund for credit note CN-000003"));
        // Pay in, pay out + reversal cancelling out, late owner withdrawal, cash drop.
        ledger.record_movement("add_cash", 1_000, false, false);
        ledger.record_movement("petty_expense", 600, false, false);
        ledger.record_movement("petty_expense", 600, true, false);
        ledger.record_movement("staff_payment", 400, false, false);
        ledger.record_movement("owner_withdrawal", 500, false, true);
        ledger.record_movement("cash_drop", 2_000, false, false);

        assert_eq!(ledger.total_cash_sales_cents, 6_500);
        assert_eq!(ledger.total_card_sales_cents, 3_000);
        assert_eq!(ledger.total_online_sales_cents, 2_000);
        assert_eq!(ledger.total_credit_sales_cents, 4_500);
        assert_eq!(ledger.total_cash_refunds_cents, 800);
        assert_eq!(ledger.total_pay_in_cents, 1_000);
        assert_eq!(ledger.total_pay_out_cents, 400);
        assert_eq!(ledger.total_owner_withdrawals_cents, 500);
        assert_eq!(ledger.total_cash_drops_cents, 2_000);
        assert!(ledger.has_late_movements);

        // 10_000 + 6_500 + 1_000 - 400 - 800 - 500 - 2_000 = 13_800
        assert_eq!(ledger.expected_cash(10_000), 13_800);
        assert_eq!(ledger.closing_cash("open", 10_000, None), (None, None));
        assert_eq!(
            ledger.closing_cash("closed", 10_000, Some(13_500)),
            (Some(13_800), Some(-300))
        );
    }

    #[test]
    fn drawer_delta_cancels_on_reversal() {
        assert_eq!(drawer_delta_for_movement("add_cash", 700, false), 700);
        assert_eq!(drawer_delta_for_movement("add_cash", 700, true), -700);
        assert_eq!(drawer_delta_for_movement("cash_drop", 700, false), -700);
        assert_eq!(drawer_delta_for_movement("cash_drop", 700, true), 700);
        assert_eq!(drawer_delta_for_movement("no_sale", 0, false), 0);
    }

    #[test]
    fn parse_rfc3339_ms_handles_z_and_offset_uniformly() {
        let z = parse_rfc3339_ms("2026-11-11T10:00:00.500Z");
        let off = parse_rfc3339_ms("2026-11-11T10:00:00.500+00:00");
        assert_eq!(z, off);
        assert!(
            parse_rfc3339_ms("2026-11-11T10:00:00.123+00:00")
                > parse_rfc3339_ms("2026-11-11T10:00:00Z")
        );
    }
}
