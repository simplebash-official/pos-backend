// Business logic for settings and shop profile persistence.

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::error::{AppError, AppResult},
    domain::settings::{ShopProfileResponse, UpdateShopProfileRequest},
    modules::{
        settings::{model::ShopProfileDocument, repository},
        tenants::repository as tenants_repository,
    },
};

pub const SHOP_PROFILE_KEY: &str = "main";

/// Fetches the shop profile for the active tenant or local installation.
/// If no profile has been saved yet, initializes with clean defaults:
/// `trading_name` set to the tenant's registered store name, and all other fields empty.
pub(crate) async fn get_shop_profile(
    db: &Db,
    tenant_id: Option<&str>,
) -> AppResult<ShopProfileResponse> {
    if let Some(doc) = repository::find_shop_profile(db, SHOP_PROFILE_KEY).await? {
        return Ok(doc.into_response());
    }

    let mut response = ShopProfileResponse::default();

    if let Some(tid) = tenant_id {
        if let Ok(Some(tenant)) = tenants_repository::find_tenant_by_key(db, tid).await {
            response.trading_name = tenant.name;
        }
    }

    Ok(response)
}

/// Updates the shop profile and persists it to the database.
pub(crate) async fn update_shop_profile(
    db: &Db,
    tenant_id: Option<&str>,
    req: UpdateShopProfileRequest,
) -> AppResult<ShopProfileResponse> {
    crate::core::logging::domain::tracked("settings.shop_profile_updated", async move {
        let existing_opt = repository::find_shop_profile(db, SHOP_PROFILE_KEY).await?;

        let mut doc = match existing_opt {
            Some(existing) => ShopProfileDocument {
                version: existing.version + 1,
                updated_at: BsonDateTime::now(),
                ..existing
            },
            None => {
                let initial_trading_name = if let Some(tid) = tenant_id {
                    tenants_repository::find_tenant_by_key(db, tid)
                        .await
                        .ok()
                        .flatten()
                        .map(|t| t.name)
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                ShopProfileDocument {
                    id: None,
                    key: SHOP_PROFILE_KEY.to_string(),
                    legal_name: String::new(),
                    trading_name: initial_trading_name,
                    address_lines: Vec::new(),
                    primary_phone: String::new(),
                    secondary_phone: String::new(),
                    email: String::new(),
                    website: String::new(),
                    business_reg_no: String::new(),
                    logo_base64: String::new(),
                    bank_name: String::new(),
                    bank_branch: String::new(),
                    account_name: String::new(),
                    account_number: String::new(),
                    default_warranty_text: String::new(),
                    default_footer_text: String::new(),
                    receipt_footer_text: String::new(),
                    version: 1,
                    created_at: BsonDateTime::now(),
                    updated_at: BsonDateTime::now(),
                }
            }
        };

        if let Some(legal_name) = req.legal_name {
            doc.legal_name = legal_name.trim().to_string();
        }

        if let Some(trading_name) = req.trading_name {
            let trimmed = trading_name.trim();
            if trimmed.is_empty() {
                return Err(AppError::validation("Trading name / store name cannot be empty"));
            }
            doc.trading_name = trimmed.to_string();

            // Keep the tenant directory's store name in sync so auth and directory reflect any rename
            if let Some(tid) = tenant_id {
                let _ = tenants_repository::update_tenant_name(db, tid, trimmed).await;
            }
        }

        if let Some(address_lines) = req.address_lines {
            doc.address_lines = address_lines
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if let Some(phone) = req.primary_phone {
            doc.primary_phone = phone.trim().to_string();
        }

        if let Some(secondary) = req.secondary_phone {
            doc.secondary_phone = secondary.trim().to_string();
        }

        if let Some(email) = req.email {
            let trimmed = email.trim();
            if !trimmed.is_empty() && !trimmed.contains('@') {
                return Err(AppError::validation("Please enter a valid email address"));
            }
            doc.email = trimmed.to_string();
        }

        if let Some(website) = req.website {
            doc.website = website.trim().to_string();
        }

        if let Some(reg_no) = req.business_reg_no {
            doc.business_reg_no = reg_no.trim().to_string();
        }

        if let Some(logo) = req.logo_base64 {
            doc.logo_base64 = logo;
        }

        if let Some(bank_name) = req.bank_name {
            doc.bank_name = bank_name.trim().to_string();
        }

        if let Some(bank_branch) = req.bank_branch {
            doc.bank_branch = bank_branch.trim().to_string();
        }

        if let Some(account_name) = req.account_name {
            doc.account_name = account_name.trim().to_string();
        }

        if let Some(account_number) = req.account_number {
            doc.account_number = account_number.trim().to_string();
        }

        if let Some(warranty) = req.default_warranty_text {
            doc.default_warranty_text = warranty;
        }

        if let Some(footer) = req.default_footer_text {
            doc.default_footer_text = footer;
        }

        if let Some(receipt_footer) = req.receipt_footer_text {
            doc.receipt_footer_text = receipt_footer;
        }

        repository::upsert_shop_profile(db, &doc).await?;

        Ok(doc.into_response())
    })
    .await
}
