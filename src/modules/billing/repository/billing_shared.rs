//! Collection handles, row mapping and the invoice filter shared by the billing repository files.

use crate::clients::sqlite::{SqlFilter, bson_to_iso, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::{AppError, AppResult};
use crate::domain::billing::{
    CreditNoteStatus, InvoiceItem, InvoiceStatus, RefundBreakdownLeg, SplitPayment,
};
use crate::modules::billing::model::{
    CreditNoteDocument, CreditNoteItemDocument, InvoiceDocument, PaymentDocument,
};

use mongodb::bson::Document;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

/// Decodes a JSON text column; stored text that is not valid JSON for `T` is an error.
fn decode_json<T: serde::de::DeserializeOwned>(column: &str, text: &str) -> AppResult<T> {
    serde_json::from_str(text)
        .map_err(|err| AppError::internal(format!("stored `{column}` is invalid: {err}")))
}

pub(crate) fn credit_note_from_sqlite_row(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<CreditNoteDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let no_receipt_int: i64 = row.get("no_receipt");
    let returned_items_str: String = row.get("returned_items");
    let returned_items: Vec<CreditNoteItemDocument> =
        decode_json("returned_items", &returned_items_str)?;
    let exchange_items_str: String = row.get("exchange_items");
    let exchange_items: Vec<InvoiceItem> = decode_json("exchange_items", &exchange_items_str)?;
    let refund_breakdown_str: String = row.get("refund_breakdown");
    let refund_breakdown: Vec<RefundBreakdownLeg> =
        decode_json("refund_breakdown", &refund_breakdown_str)?;
    let refund_payment_keys_str: String = row.get("refund_payment_keys");
    let refund_payment_keys: Vec<String> =
        decode_json("refund_payment_keys", &refund_payment_keys_str)?;
    let status_str: String = row.get("status");
    let status: CreditNoteStatus = serde_json::from_value(serde_json::Value::String(status_str))
        .map_err(|err| {
            AppError::internal(format!("stored credit note status is invalid: {err}"))
        })?;
    let is_manager_override_int: i64 = row.get("is_manager_override");

    let voided_at_str: Option<String> = row.get("voided_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(CreditNoteDocument {
        id,
        key: row.get("key"),
        credit_note_number: row.get("credit_note_number"),
        invoice_key: row.get("invoice_key"),
        invoice_number: row.get("invoice_number"),
        no_receipt: no_receipt_int != 0,
        customer_key: row.get("customer_key"),
        customer_name_snapshot: row.get("customer_name_snapshot"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        returned_items,
        exchange_items,
        exchange_reference: row.get("exchange_reference"),
        return_subtotal_cents: row.get("return_subtotal_cents"),
        exchange_subtotal_cents: row.get("exchange_subtotal_cents"),
        net_refund_cents: row.get("net_refund_cents"),
        refund_cash_cents: row.get("refund_cash_cents"),
        balance_reduction_cents: row.get("balance_reduction_cents"),
        refund_breakdown,
        refund_payment_keys,
        status,
        is_manager_override: is_manager_override_int != 0,
        override_approved_by: row.get("override_approved_by"),
        override_reason: row.get("override_reason"),
        notes: row.get("notes"),
        session_key: row.get("session_key"),
        voided_at: voided_at_str.map(|s| to_bson_datetime(&s)).transpose()?,
        voided_by: row.get("voided_by"),
        voided_reason: row.get("voided_reason"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str)?,
        updated_at: to_bson_datetime(&updated_at_str)?,
    })
}

pub(crate) fn invoice_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<InvoiceDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let items_str: String = row.get("items");
    let items: Vec<InvoiceItem> = decode_json("items", &items_str)?;
    let split_payments_str: Option<String> = row.get("split_payments");
    let split_payments: Option<Vec<SplitPayment>> = split_payments_str
        .map(|text| decode_json("split_payments", &text))
        .transpose()?;
    let is_credit_int: i64 = row.get("is_credit");
    let status_str: String = row.get("status");
    let status: InvoiceStatus = serde_json::from_value(serde_json::Value::String(status_str))
        .map_err(|err| AppError::internal(format!("stored invoice status is invalid: {err}")))?;
    let shop_profile_str: String = row.get("shop_profile_snapshot");
    let shop_profile_snapshot: serde_json::Value =
        decode_json("shop_profile_snapshot", &shop_profile_str)?;

    let voided_at_str: Option<String> = row.get("voided_at");
    let closed_at_str: Option<String> = row.get("closed_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(InvoiceDocument {
        id,
        key: row.get("key"),
        invoice_number: row.get("invoice_number"),
        customer_key: row.get("customer_key"),
        customer_name_snapshot: row.get("customer_name_snapshot"),
        customer_phone_snapshot: row.get("customer_phone_snapshot"),
        customer_address_snapshot: row.get("customer_address_snapshot"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        items,
        subtotal_cents: row.get("subtotal_cents"),
        discount_type: row.get("discount_type"),
        discount_value: row.get("discount_value"),
        discount_cents: row.get("discount_cents"),
        total_cents: row.get("total_cents"),
        payment_method: row.get("payment_method"),
        split_payments,
        is_credit: is_credit_int != 0,
        amount_received_cents: row.get("amount_received_cents"),
        change_due_cents: row.get("change_due_cents"),
        due_date: row.get("due_date"),
        card_last4: row.get("card_last4"),
        card_ref: row.get("card_ref"),
        online_ref: row.get("online_ref"),
        online_note: row.get("online_note"),
        status,
        notes: row.get("notes"),
        shop_profile_snapshot,
        warranty_terms_snapshot: row.get("warranty_terms_snapshot"),
        document_selection: row.get("document_selection"),
        voided_at: voided_at_str.map(|s| to_bson_datetime(&s)).transpose()?,
        voided_by: row.get("voided_by"),
        voided_reason: row.get("voided_reason"),
        closed_at: closed_at_str.map(|s| to_bson_datetime(&s)).transpose()?,
        closed_by: row.get("closed_by"),
        refunded_cents: row.get("refunded_cents"),
        credit_note_count: row.get("credit_note_count"),
        session_key: row.get("session_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str)?,
        updated_at: to_bson_datetime(&updated_at_str)?,
        updated_by_device: row.get("updated_by_device"),
    })
}

pub(super) fn invoices(db: &TenantDatabase) -> ScopedCollection<InvoiceDocument> {
    db.collection("invoices")
}

pub(super) fn build_invoice_where_clause(filter: &Document) -> SqlFilter {
    let mut conditions = vec!["deleted_at IS NULL".to_string()];
    let mut bindings = Vec::new();

    fn process_clause(doc: &Document, conditions: &mut Vec<String>, bindings: &mut Vec<String>) {
        if let Ok(key) = doc.get_str("key") {
            conditions.push("key = ?".to_string());
            bindings.push(key.to_string());
        }
        if let Ok(customer_key) = doc.get_str("customer_key") {
            conditions.push("customer_key = ?".to_string());
            bindings.push(customer_key.to_string());
        }
        if let Ok(status) = doc.get_str("status") {
            conditions.push("status = ?".to_string());
            bindings.push(status.to_string());
        }
        if let Ok(payment_method) = doc.get_str("payment_method") {
            conditions.push("payment_method = ?".to_string());
            bindings.push(payment_method.to_string());
        }
        if let Ok(session_key) = doc.get_str("session_key") {
            conditions.push("session_key = ?".to_string());
            bindings.push(session_key.to_string());
        }
        if let Ok(is_credit) = doc.get_bool("is_credit") {
            conditions.push("is_credit = ?".to_string());
            bindings.push(if is_credit {
                "1".to_string()
            } else {
                "0".to_string()
            });
        }
        if let Ok(created_at_doc) = doc.get_document("created_at") {
            if let Ok(gte) = created_at_doc.get_datetime("$gte") {
                conditions.push("created_at >= ?".to_string());
                bindings.push(bson_to_iso(gte));
            }
            if let Ok(lt) = created_at_doc.get_datetime("$lt") {
                conditions.push("created_at < ?".to_string());
                bindings.push(bson_to_iso(lt));
            }
        }
        if let Ok(or_arr) = doc.get_array("$or") {
            let mut or_parts = Vec::new();
            for or_item in or_arr {
                if let Some(or_doc) = or_item.as_document() {
                    if let Ok(inv_num) = or_doc.get_document("invoice_number") {
                        if let Ok(regex) = inv_num.get_str("$regex") {
                            let clean = regex.trim_start_matches('^').trim_end_matches('$');
                            or_parts.push("invoice_number LIKE ?".to_string());
                            bindings.push(format!("%{clean}%"));
                        }
                    } else if let Ok(inv_exact) = or_doc.get_str("invoice_number") {
                        or_parts.push("invoice_number = ?".to_string());
                        bindings.push(inv_exact.to_string());
                    }
                    if let Ok(cname) = or_doc.get_document("customer_name_snapshot")
                        && let Ok(regex) = cname.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_name_snapshot LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }
                    if let Ok(cphone) = or_doc.get_document("customer_phone_snapshot")
                        && let Ok(regex) = cphone.get_str("$regex")
                    {
                        let clean = regex.trim_start_matches('^').trim_end_matches('$');
                        or_parts.push("customer_phone_snapshot LIKE ?".to_string());
                        bindings.push(format!("%{clean}%"));
                    }
                    if let Ok(is_cred) = or_doc.get_bool("is_credit") {
                        or_parts.push("is_credit = ?".to_string());
                        bindings.push(if is_cred {
                            "1".to_string()
                        } else {
                            "0".to_string()
                        });
                    }
                    if let Ok(st) = or_doc.get_str("status") {
                        or_parts.push("status = ?".to_string());
                        bindings.push(st.to_string());
                    }
                }
            }
            if !or_parts.is_empty() {
                conditions.push(format!("({})", or_parts.join(" OR ")));
            }
        }
    }

    if let Ok(and_arr) = filter.get_array("$and") {
        for item in and_arr {
            if let Some(doc) = item.as_document() {
                process_clause(doc, &mut conditions, &mut bindings);
            }
        }
    } else {
        process_clause(filter, &mut conditions, &mut bindings);
    }

    SqlFilter {
        clause: conditions.join(" AND "),
        bindings,
    }
}

pub(crate) fn payment_from_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<PaymentDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let recorded_at_str: String = row.get("recorded_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(PaymentDocument {
        id,
        key: row.get("key"),
        invoice_key: row.get("invoice_key"),
        amount_cents: row.get("amount_cents"),
        payment_method: row.get("payment_method"),
        notes: row.get("notes"),
        session_key: row.get("session_key"),
        recorded_by_user_id: row.get("recorded_by_user_id"),
        recorded_by_name_snapshot: row.get("recorded_by_name_snapshot"),
        recorded_at: to_bson_datetime(&recorded_at_str)?,
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str)?,
        updated_at: to_bson_datetime(&updated_at_str)?,
    })
}

pub(super) fn payments(db: &TenantDatabase) -> ScopedCollection<PaymentDocument> {
    db.collection("payments")
}
