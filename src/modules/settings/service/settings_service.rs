use crate::clients::db::Db;
use crate::clients::document_server::DocumentServerClient;
use crate::clients::uow::Unit;
use crate::core::constants::codes;
use crate::core::error::AppError;
use crate::core::error::AppResult;
use crate::domain::settings::DocumentTemplatesResponse;
use crate::domain::settings::ShopProfileResponse;
use crate::domain::settings::UpdateDocumentTemplatesRequest;
use crate::domain::settings::UpdateShopProfileRequest;
use crate::modules::documents::service::templates;
use crate::modules::documents::service::templates::DesignSettings;
use crate::modules::settings::model::DocumentTemplateSettingsDocument;
use crate::modules::settings::model::ShopProfileDocument;
use crate::modules::settings::repository;
use crate::modules::tenants::repository as tenants_repository;
use mongodb::bson::DateTime as BsonDateTime;

pub(super) const DOCUMENT_TEMPLATE_SETTINGS_KEY: &str = "main";

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

/// Initializes the single `shop_profiles` row for a shop if it does not yet exist.
/// Modeled on `cash_settings::service::ensure_settings`: missing shop profile is an internal error,
/// never a silent code default.
pub async fn ensure_shop_profile(db: &Db, initial_trading_name: Option<&str>) -> AppResult<()> {
    db.require_active_tenant_scope()?;
    let seeded_at = BsonDateTime::from_millis(1_767_225_600_000);
    let initial_name = match initial_trading_name {
        Some(name) => name.trim(),
        None => "",
    };

    if let Some(mut existing) = repository::find_shop_profile(db, SHOP_PROFILE_KEY).await? {
        if existing.trading_name.trim().is_empty() && !initial_name.is_empty() {
            existing.trading_name = initial_name.to_string();
            existing.version += 1;
            existing.updated_at = BsonDateTime::now();
            repository::upsert_shop_profile(db, &existing).await?;
        }
        return Ok(());
    }

    let doc = ShopProfileDocument {
        id: None,
        key: SHOP_PROFILE_KEY.to_string(),
        legal_name: String::new(),
        trading_name: initial_name.to_string(),
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
        allow_staff_multi_device_sessions: false,
        version: 1,
        created_at: seeded_at,
        updated_at: seeded_at,
        updated_by_device: None,
    };
    repository::insert_shop_profile_if_absent(db, &doc).await
}

/// Fetches the shop profile for the active shop. Fails if the row has not been initialized.
pub(crate) async fn get_shop_profile<'a>(
    unit: impl Into<Unit<'a>>,
) -> AppResult<ShopProfileResponse> {
    let doc = repository::find_shop_profile(unit, SHOP_PROFILE_KEY)
        .await?
        .ok_or_else(|| AppError::internal("Shop profile has not been initialized for this shop"))?;
    Ok(doc.into_response())
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

pub(super) async fn persist_document_template_settings(
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

/// PDF of one design's sample data, for the Settings preview.
pub(crate) async fn render_document_template_preview(
    doc_server: &DocumentServerClient,
    family: &str,
) -> AppResult<Vec<u8>> {
    templates::render_preview(doc_server, family).await
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

pub const SHOP_PROFILE_KEY: &str = "main";

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

/// Updates the shop profile and persists it to the database transactionally.
pub(crate) async fn update_shop_profile(
    db: &Db,
    tenant_id: Option<&str>,
    req: UpdateShopProfileRequest,
) -> AppResult<ShopProfileResponse> {
    crate::core::logging::domain::tracked("settings.shop_profile_updated", async move {
        let mut tx = db.begin().await?;
        let existing = repository::find_shop_profile(&mut tx, SHOP_PROFILE_KEY)
            .await?
            .ok_or_else(|| {
                AppError::internal("Shop profile has not been initialized for this shop")
            })?;

        let mut doc = ShopProfileDocument {
            version: existing.version + 1,
            updated_at: BsonDateTime::now(),
            ..existing
        };
        let previous_allow_shared = doc.allow_shared_user_sessions;
        let previous_allow_staff_multi = doc.allow_staff_multi_device_sessions;
        let mut tenant_name_update: Option<String> = None;

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

            if tenant_id.is_some() {
                tenant_name_update = Some(trimmed.to_string());
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

        let next_allow_shared = match req.allow_shared_user_sessions {
            Some(v) => v,
            None => previous_allow_shared,
        };
        let next_allow_staff_multi = match req.allow_staff_multi_device_sessions {
            Some(v) => v,
            None => previous_allow_staff_multi,
        };
        let shared_changed = next_allow_shared != previous_allow_shared;
        let staff_multi_changed = next_allow_staff_multi != previous_allow_staff_multi;

        if next_allow_shared && staff_multi_changed {
            return Err(AppError::validation(
                "Staff multi-device setting applies only in Personal Shifts mode.",
            ));
        }

        if shared_changed || staff_multi_changed {
            let active_count =
                crate::modules::sessions::repository::sessions::count_active_sessions(&mut tx)
                    .await?;
            if active_count > 0 {
                let conflict_msg = if shared_changed {
                    "Cannot change register shift mode while active register shifts exist. Please close all open, closing, or reopened shifts first."
                } else {
                    "Cannot change staff multi-device setting while active register shifts exist. Please close all open, closing, or reopened shifts first."
                };
                return Err(AppError::conflict(
                    codes::ACTIVE_SESSIONS_EXIST,
                    conflict_msg,
                ));
            }
        }

        if shared_changed {
            let drawer_states =
                crate::modules::sessions::repository::drawer::list_drawer_states(&mut tx).await?;
            let total_balance: i64 = drawer_states.iter().map(|s| s.current_balance_cents).sum();
            if total_balance != 0 {
                return Err(AppError::conflict(
                    codes::DRAWER_BALANCE_NONZERO,
                    format!(
                        "Cannot change register shift mode while cash drawers hold {total_balance} cents. Drop all drawer cash to safe first."
                    ),
                ));
            }
        }

        doc.allow_shared_user_sessions = next_allow_shared;
        doc.allow_staff_multi_device_sessions = next_allow_staff_multi;

        repository::upsert_shop_profile(&mut tx, &doc).await?;
        tx.commit().await?;

        // Keep the tenant directory's store name in sync outside the shop transaction.
        if let (Some(tid), Some(name)) = (tenant_id, tenant_name_update) {
            tenants_repository::update_tenant_name(db, tid, &name).await?;
        }

        Ok(doc.into_response())
    })
    .await
}
