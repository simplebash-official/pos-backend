// Z-Report (End-of-Session Summary) compilation service.

use chrono::Utc;

use crate::{
    clients::db::Db,
    core::{
        constants::codes,
        error::{AppError, AppResult},
    },
    domain::sessions::{
        ZReportCashReconciliation, ZReportPaymentMethodSummary, ZReportResponse,
    },
    modules::sessions::repository::{movements, sessions},
};

pub async fn generate_z_report(db: &Db, session_key: &str) -> AppResult<ZReportResponse> {
    let session_doc = sessions::find_session_by_id_or_key(db, session_key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Register session not found", codes::SESSION_NOT_FOUND))?;

    let session_movements = movements::list_cash_movements_for_session(db, &session_doc.key).await?;

    let expected_cash = session_doc.opening_float_cents
        + session_doc.total_cash_sales_cents
        + session_doc.total_pay_in_cents
        - session_doc.total_pay_out_cents
        - session_doc.total_cash_refunds_cents
        - session_doc.total_owner_withdrawals_cents
        - session_doc.total_cash_drops_cents;

    let counted_cash = session_doc.counted_cash_cents.unwrap_or(expected_cash);
    let variance = session_doc.cash_variance_cents.unwrap_or(counted_cash - expected_cash);

    let gross_sales = session_doc.total_cash_sales_cents
        + session_doc.total_card_sales_cents
        + session_doc.total_online_sales_cents
        + session_doc.total_credit_sales_cents;

    let returns = session_doc.total_cash_refunds_cents;
    let net_sales = gross_sales - returns;

    let mut payment_methods = Vec::new();
    if session_doc.total_cash_sales_cents > 0 {
        payment_methods.push(ZReportPaymentMethodSummary {
            method: "cash".to_string(),
            count: 0,
            total_cents: session_doc.total_cash_sales_cents,
        });
    }
    if session_doc.total_card_sales_cents > 0 {
        payment_methods.push(ZReportPaymentMethodSummary {
            method: "card".to_string(),
            count: 0,
            total_cents: session_doc.total_card_sales_cents,
        });
    }
    if session_doc.total_online_sales_cents > 0 {
        payment_methods.push(ZReportPaymentMethodSummary {
            method: "online".to_string(),
            count: 0,
            total_cents: session_doc.total_online_sales_cents,
        });
    }
    if session_doc.total_credit_sales_cents > 0 {
        payment_methods.push(ZReportPaymentMethodSummary {
            method: "credit".to_string(),
            count: 0,
            total_cents: session_doc.total_credit_sales_cents,
        });
    }

    let cash_reconciliation = ZReportCashReconciliation {
        opening_float_cents: session_doc.opening_float_cents,
        cash_sales_cents: session_doc.total_cash_sales_cents,
        pay_in_cents: session_doc.total_pay_in_cents,
        pay_out_cents: session_doc.total_pay_out_cents,
        cash_refunds_cents: session_doc.total_cash_refunds_cents,
        owner_withdrawals_cents: session_doc.total_owner_withdrawals_cents,
        cash_drops_cents: session_doc.total_cash_drops_cents,
        expected_cash_cents: expected_cash,
        counted_cash_cents: counted_cash,
        variance_cents: variance,
        closing_float_cents: session_doc.closing_float_cents.unwrap_or(0),
        safe_deposit_cents: session_doc.cash_drop_to_safe_cents.unwrap_or(0),
    };

    Ok(ZReportResponse {
        session: session_doc.into_register_session(),
        gross_sales_cents: gross_sales,
        returns_cents: returns,
        net_sales_cents: net_sales,
        discount_cents: 0,
        total_invoices_count: 0,
        payment_methods,
        cash_movements: session_movements.into_iter().map(|m| m.into_cash_movement()).collect(),
        cash_reconciliation,
        generated_at: Utc::now(),
    })
}
