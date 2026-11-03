use super::warranty_document::stored_day;
use crate::core::error::{AppError, AppResult};
use crate::domain::warranties::{
    ClaimDecision, ClaimStatus, SupplierClaim, VoidChecks, WarrantyClaim,
};
use chrono::NaiveDate;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// How a warranty claim is stored. Top-level names are the API's in snake_case and the nested
/// `void_checks` and `supplier` keep their camelCase, so the sync codec carries it unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarrantyClaimDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub claim_number: String,
    pub warranty_key: String,
    pub warranty_number: String,
    pub item_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub invoice_key: String,
    pub invoice_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub issue_description: String,
    pub void_checks: VoidChecks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    pub status: ClaimStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspection_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<ClaimDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired_override_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_warranty_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_note_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_repair_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_repair_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<SupplierClaim>,
    /// `YYYY-MM-DD`, shop-local.
    pub received_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_on: Option<String>,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl WarrantyClaimDocument {
    /// The API shape, with its age and supplier lateness worked out for `today` (shop-local).
    pub fn into_claim(self, today: NaiveDate) -> AppResult<WarrantyClaim> {
        let received_on = stored_day("received_on", &self.received_on)?;
        let supplier_overdue = match &self.supplier {
            Some(SupplierClaim {
                expected_back_on: Some(expected),
                received_back_on: None,
                ..
            }) if self.status == ClaimStatus::AtSupplier => {
                stored_day("expected_back_on", expected)? < today
            }
            _ => false,
        };
        Ok(WarrantyClaim {
            id: self
                .id
                .ok_or_else(|| AppError::internal("persisted warranty claim must have an id"))?
                .to_hex(),
            key: self.key,
            claim_number: self.claim_number,
            warranty_key: self.warranty_key,
            warranty_number: self.warranty_number,
            item_name: self.item_name,
            serial_number: self.serial_number,
            product_key: self.product_key,
            invoice_key: self.invoice_key,
            invoice_number: self.invoice_number,
            customer_key: self.customer_key,
            customer_name: self.customer_name,
            customer_phone: self.customer_phone,
            issue_description: self.issue_description,
            void_checks: self.void_checks,
            assigned_employee_key: self.assigned_employee_key,
            assigned_employee_name: self.assigned_employee_name,
            status: self.status,
            inspection_notes: self.inspection_notes,
            decision: self.decision,
            decision_reason: self.decision_reason,
            decided_by_user_id: self.decided_by_user_id,
            decided_by_name: self.decided_by_name,
            expired_override_by_name: self.expired_override_by_name,
            resolution_note: self.resolution_note,
            replacement_serial_number: self.replacement_serial_number,
            replacement_warranty_key: self.replacement_warranty_key,
            credit_note_key: self.credit_note_key,
            paid_repair_key: self.paid_repair_key,
            paid_repair_number: self.paid_repair_number,
            supplier: self.supplier,
            received_on: self.received_on,
            resolved_on: self.resolved_on,
            returned_on: self.returned_on,
            age_days: (today - received_on).num_days(),
            supplier_overdue,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            updated_by_device: self.updated_by_device,
        })
    }
}
