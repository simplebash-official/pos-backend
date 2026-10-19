// Business logic for settings and shop profile persistence.

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::{db::Db, document_server::DocumentServerClient},
    core::error::{AppError, AppResult},
    domain::settings::{
        DocumentTemplatesResponse, ShopProfileResponse, UpdateDocumentTemplatesRequest,
        UpdateShopProfileRequest,
    },
    modules::{
        documents::service::templates::{self, DesignSettings},
        settings::{
            model::{DocumentTemplateSettingsDocument, ShopProfileDocument},
            repository,
        },
        tenants::repository as tenants_repository,
    },
};

pub const SHOP_PROFILE_KEY: &str = "main";
const DOCUMENT_TEMPLATE_SETTINGS_KEY: &str = "main";

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
        response.trading_name = tenants_repository::find_tenant_by_key(db, tid)
            .await
            .ok()
            .flatten()
            .map(|t| t.name)
            .unwrap_or_default();
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
                    allow_shared_user_sessions: false,
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
                return Err(AppError::validation(
                    "Trading name / store name cannot be empty",
                ));
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

        if let Some(new_val) = req.allow_shared_user_sessions {
            if new_val != doc.allow_shared_user_sessions {
                let active_count =
                    crate::modules::sessions::repository::sessions::count_active_sessions(db)
                        .await?;
                if active_count > 0 {
                    return Err(AppError::conflict(
                        crate::core::constants::codes::ACTIVE_SESSIONS_EXIST,
                        "Cannot change register shift mode while active register shifts exist. Please close all open shifts in the shop first.",
                    ));
                }
                doc.allow_shared_user_sessions = new_val;
            }
        }

        repository::upsert_shop_profile(db, &doc).await?;

        Ok(doc.into_response())
    })
    .await
}

/// The shop's stored designs: the family chosen per document kind and the
/// option values saved per design (both empty when the shop never changed
/// anything). Printing reads this to decide which design to render.
pub(crate) async fn get_document_template_settings(db: &Db) -> AppResult<DesignSettings> {
    Ok(
        repository::find_document_template_settings(db, DOCUMENT_TEMPLATE_SETTINGS_KEY)
            .await?
            .map(|doc| DesignSettings {
                selections: doc.selections,
                options: doc.options,
            })
            .unwrap_or_default(),
    )
}

/// Every selectable document kind with the designs available for it, the
/// shop's current choice and the option values in force.
pub(crate) async fn get_document_templates(
    db: &Db,
    doc_server: &DocumentServerClient,
) -> AppResult<DocumentTemplatesResponse> {
    let settings = get_document_template_settings(db).await?;
    Ok(DocumentTemplatesResponse {
        kinds: templates::list(doc_server, &settings).await?,
    })
}

/// Changes the shop's designs and option values. Every pick and value is
/// validated against the document-server's current templates first; nothing
/// is saved unless all of it is valid.
pub(crate) async fn update_document_templates(
    db: &Db,
    doc_server: &DocumentServerClient,
    req: UpdateDocumentTemplatesRequest,
) -> AppResult<DocumentTemplatesResponse> {
    crate::core::logging::domain::tracked("settings.document_templates_updated", async move {
        let existing =
            repository::find_document_template_settings(db, DOCUMENT_TEMPLATE_SETTINGS_KEY).await?;
        let current = existing
            .as_ref()
            .map(|doc| DesignSettings {
                selections: doc.selections.clone(),
                options: doc.options.clone(),
            })
            .unwrap_or_default();

        let merged =
            templates::merge_settings(doc_server, current, &req.selections, &req.options).await?;

        persist_document_template_settings(db, existing, &merged).await?;
        Ok(DocumentTemplatesResponse {
            kinds: templates::list(doc_server, &merged).await?,
        })
    })
    .await
}

async fn persist_document_template_settings(
    db: &Db,
    existing: Option<DocumentTemplateSettingsDocument>,
    settings: &DesignSettings,
) -> AppResult<()> {
    let now = BsonDateTime::now();
    let doc = match existing {
        Some(existing) => DocumentTemplateSettingsDocument {
            selections: settings.selections.clone(),
            options: settings.options.clone(),
            version: existing.version + 1,
            updated_at: now,
            ..existing
        },
        None => DocumentTemplateSettingsDocument {
            id: None,
            key: DOCUMENT_TEMPLATE_SETTINGS_KEY.to_string(),
            selections: settings.selections.clone(),
            options: settings.options.clone(),
            version: 1,
            created_at: now,
            updated_at: now,
        },
    };
    repository::upsert_document_template_settings(db, &doc).await
}

/// The kind/family pairs of this shop's stored selections (admin console).
pub async fn list_document_selections(db: &Db) -> AppResult<Vec<(String, String)>> {
    let mut selections: Vec<(String, String)> = get_document_template_settings(db)
        .await?
        .selections
        .into_iter()
        .collect();
    selections.sort();
    Ok(selections)
}

/// Removes every stored selection of the given families and the option
/// values saved for them; returns the `(kind, family)` selections removed.
/// An explicit operator action: printing then needs a new choice (or falls
/// to the standard design when the kind is left unchosen).
pub async fn reset_document_families(
    db: &Db,
    families: &[String],
) -> AppResult<Vec<(String, String)>> {
    crate::core::logging::domain::tracked("settings.document_designs_reset", async move {
        let existing =
            repository::find_document_template_settings(db, DOCUMENT_TEMPLATE_SETTINGS_KEY).await?;
        let Some(existing) = existing else {
            return Ok(Vec::new());
        };
        let mut settings = DesignSettings {
            selections: existing.selections.clone(),
            options: existing.options.clone(),
        };
        let mut removed: Vec<(String, String)> = settings
            .selections
            .iter()
            .filter(|(_, family)| families.contains(family))
            .map(|(kind, family)| (kind.clone(), family.clone()))
            .collect();
        removed.sort();
        settings
            .selections
            .retain(|_, family| !families.contains(family));
        settings
            .options
            .retain(|family, _| !families.contains(family));
        persist_document_template_settings(db, Some(existing), &settings).await?;
        Ok(removed)
    })
    .await
}

/// PDF of one design's sample data, for the Settings preview.
pub(crate) async fn render_document_template_preview(
    doc_server: &DocumentServerClient,
    family: &str,
) -> AppResult<Vec<u8>> {
    templates::render_preview(doc_server, family).await
}
