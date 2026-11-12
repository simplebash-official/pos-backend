use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::clients::sqlite::generate_id_hex;
use crate::clients::sqlite::to_bson_datetime;
use crate::clients::uow::Backend;
use crate::clients::uow::Unit;
use crate::core::error::AppError;
use crate::core::error::AppResult;
use crate::modules::settings::model::DocumentTemplateSettingsDocument;
use crate::modules::settings::model::ShopProfileDocument;
use mongodb::bson::Bson;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) const DOCUMENT_TEMPLATE_SETTINGS: &str = "document_template_settings";

pub(crate) async fn find_document_template_settings(
    db: &Db,
    key: &str,
) -> AppResult<Option<DocumentTemplateSettingsDocument>> {
    let fetch_err = |e: &dyn std::fmt::Display| {
        AppError::internal(format!("Failed to fetch document template settings: {e}"))
    };
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT key, selections, options, version, created_at, updated_at \
                 FROM document_template_settings WHERE key = ?1 LIMIT 1",
            )
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|e| fetch_err(&e))?;

            row.map(|r| -> AppResult<_> {
                let selections_json: String = r.get("selections");
                let options_json: String = r.get("options");
                let created_at: String = r.get("created_at");
                let updated_at: String = r.get("updated_at");
                Ok(DocumentTemplateSettingsDocument {
                    id: None,
                    key: r.get("key"),
                    selections: serde_json::from_str(&selections_json).map_err(|err| {
                        AppError::internal(format!("stored selections are not JSON: {err}"))
                    })?,
                    options: serde_json::from_str(&options_json).map_err(|err| {
                        AppError::internal(format!("stored options are not JSON: {err}"))
                    })?,
                    version: r.get("version"),
                    created_at: to_bson_datetime(&created_at)?,
                    updated_at: to_bson_datetime(&updated_at)?,
                })
            })
            .transpose()
        }
        Db::Mongo(mongo) => mongo
            .collection::<DocumentTemplateSettingsDocument>(DOCUMENT_TEMPLATE_SETTINGS)
            .find_one(doc! { "key": key })
            .await
            .map_err(|e| fetch_err(&e)),
    }
}

pub(crate) async fn find_shop_profile<'a>(
    unit: impl Into<Unit<'a>>,
    key: &str,
) -> AppResult<Option<ShopProfileDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Sqlite(mut conn) => {
            let row = sqlx::query(
                "SELECT key, id, legal_name, trading_name, address_lines, primary_phone, \
                 secondary_phone, email, website, business_reg_no, logo_base64, bank_name, \
                 bank_branch, account_name, account_number, default_warranty_text, \
                 default_footer_text, receipt_footer_text, allow_shared_user_sessions, \
                 allow_staff_multi_device_sessions, version, created_at, updated_at, \
                 updated_by_device \
                 FROM shop_profiles WHERE key = ?1 LIMIT 1",
            )
            .bind(key)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| AppError::internal(format!("Failed to fetch shop profile: {e}")))?;

            row.map(|r| -> AppResult<_> {
                let address_lines_str: String = r.get("address_lines");
                let address_lines: Vec<String> =
                    serde_json::from_str(&address_lines_str).map_err(|err| {
                        AppError::internal(format!("stored address lines are not JSON: {err}"))
                    })?;
                let created_at: String = r.get("created_at");
                let updated_at: String = r.get("updated_at");
                let id_str: String = r.get("id");
                let allow_shared_user_sessions_int: i64 =
                    r.try_get("allow_shared_user_sessions")?;
                let allow_staff_multi_device_sessions_int: i64 =
                    r.try_get("allow_staff_multi_device_sessions")?;

                Ok(ShopProfileDocument {
                    id: ObjectId::parse_str(&id_str).ok(),
                    key: r.get("key"),
                    legal_name: r.get("legal_name"),
                    trading_name: r.get("trading_name"),
                    address_lines,
                    primary_phone: r.get("primary_phone"),
                    secondary_phone: r.get("secondary_phone"),
                    email: r.get("email"),
                    website: r.get("website"),
                    business_reg_no: r.get("business_reg_no"),
                    logo_base64: r.get("logo_base64"),
                    bank_name: r.get("bank_name"),
                    bank_branch: r.get("bank_branch"),
                    account_name: r.get("account_name"),
                    account_number: r.get("account_number"),
                    default_warranty_text: r.get("default_warranty_text"),
                    default_footer_text: r.get("default_footer_text"),
                    receipt_footer_text: r.get("receipt_footer_text"),
                    allow_shared_user_sessions: allow_shared_user_sessions_int != 0,
                    allow_staff_multi_device_sessions: allow_staff_multi_device_sessions_int != 0,
                    version: r.get("version"),
                    created_at: to_bson_datetime(&created_at)?,
                    updated_at: to_bson_datetime(&updated_at)?,
                    updated_by_device: r.get("updated_by_device"),
                })
            })
            .transpose()
        }
        Backend::Mongo(mongo, session) => {
            let collection = mongo.collection::<ShopProfileDocument>("shop_profiles");
            let doc_opt = crate::in_session!(collection.find_one(doc! { "key": key }), session)
                .map_err(|e| AppError::internal(format!("Failed to fetch shop profile: {e}")))?;
            Ok(doc_opt)
        }
    }
}

pub(crate) async fn upsert_document_template_settings(
    db: &Db,
    doc: &DocumentTemplateSettingsDocument,
) -> AppResult<()> {
    let save_err = |e: &dyn std::fmt::Display| {
        AppError::internal(format!("Failed to save document template settings: {e}"))
    };
    match db {
        Db::Sqlite(pool) => {
            let selections_json =
                serde_json::to_string(&doc.selections).map_err(|e| save_err(&e))?;
            let options_json = serde_json::to_string(&doc.options).map_err(|e| save_err(&e))?;
            sqlx::query(
                "INSERT INTO document_template_settings (key, selections, options, version, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(key) DO UPDATE SET \
                    selections = excluded.selections, \
                    options = excluded.options, \
                    version = excluded.version, \
                    updated_at = excluded.updated_at",
            )
            .bind(&doc.key)
            .bind(&selections_json)
            .bind(&options_json)
            .bind(doc.version)
            .bind(bson_to_iso(&doc.created_at))
            .bind(bson_to_iso(&doc.updated_at))
            .execute(pool)
            .await
            .map_err(|e| save_err(&e))?;
            Ok(())
        }
        Db::Mongo(mongo) => {
            let selections =
                mongodb::bson::serialize_to_bson(&doc.selections).map_err(|e| save_err(&e))?;
            let options =
                mongodb::bson::serialize_to_bson(&doc.options).map_err(|e| save_err(&e))?;
            mongo
                .collection::<mongodb::bson::Document>(DOCUMENT_TEMPLATE_SETTINGS)
                .update_one(
                    doc! { "key": &doc.key },
                    doc! {
                        "$set": {
                            "key": &doc.key,
                            "selections": selections,
                            "options": options,
                            "version": doc.version,
                            "updated_at": doc.updated_at,
                        },
                        "$setOnInsert": { "created_at": doc.created_at },
                    },
                )
                .upsert(true)
                .await
                .map_err(|e| save_err(&e))?;
            Ok(())
        }
    }
}

/// Inserts the shop profile only when absent, safe under concurrent startup/provisioning races.
pub(crate) async fn insert_shop_profile_if_absent(
    db: &Db,
    doc: &ShopProfileDocument,
) -> AppResult<()> {
    match db {
        Db::Sqlite(pool) => {
            let address_lines_json = serde_json::to_string(&doc.address_lines)?;
            let created_at = bson_to_iso(&doc.created_at);
            let updated_at = bson_to_iso(&doc.updated_at);
            let id_hex = match doc.id {
                Some(oid) => oid.to_hex(),
                None => generate_id_hex(),
            };

            sqlx::query(
                "INSERT INTO shop_profiles ( \
                    key, id, legal_name, trading_name, address_lines, primary_phone, secondary_phone, \
                    email, website, business_reg_no, logo_base64, bank_name, bank_branch, account_name, \
                    account_number, default_warranty_text, default_footer_text, receipt_footer_text, \
                    allow_shared_user_sessions, allow_staff_multi_device_sessions, version, created_at, \
                    updated_at, updated_by_device \
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24) \
                ON CONFLICT(key) DO NOTHING",
            )
            .bind(&doc.key)
            .bind(&id_hex)
            .bind(&doc.legal_name)
            .bind(&doc.trading_name)
            .bind(&address_lines_json)
            .bind(&doc.primary_phone)
            .bind(&doc.secondary_phone)
            .bind(&doc.email)
            .bind(&doc.website)
            .bind(&doc.business_reg_no)
            .bind(&doc.logo_base64)
            .bind(&doc.bank_name)
            .bind(&doc.bank_branch)
            .bind(&doc.account_name)
            .bind(&doc.account_number)
            .bind(&doc.default_warranty_text)
            .bind(&doc.default_footer_text)
            .bind(&doc.receipt_footer_text)
            .bind(if doc.allow_shared_user_sessions { 1i64 } else { 0i64 })
            .bind(if doc.allow_staff_multi_device_sessions {
                1i64
            } else {
                0i64
            })
            .bind(doc.version)
            .bind(&created_at)
            .bind(&updated_at)
            .bind(&doc.updated_by_device)
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to initialize shop profile: {e}")))?;

            Ok(())
        }
        Db::Mongo(mongo) => {
            let collection = mongo.collection::<mongodb::bson::Document>("shop_profiles");
            let bson_doc = mongodb::bson::serialize_to_document(doc).map_err(|e| {
                AppError::internal(format!("Failed to encode shop profile document: {e}"))
            })?;
            collection
                .update_one(doc! { "key": &doc.key }, doc! { "$setOnInsert": bson_doc })
                .upsert(true)
                .await
                .map_err(|e| {
                    AppError::internal(format!("Failed to initialize shop profile: {e}"))
                })?;
            Ok(())
        }
    }
}

pub(crate) async fn upsert_shop_profile<'a>(
    unit: impl Into<Unit<'a>>,
    doc: &ShopProfileDocument,
) -> AppResult<()> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Sqlite(mut conn) => {
            let address_lines_json = serde_json::to_string(&doc.address_lines)?;
            let created_at = bson_to_iso(&doc.created_at);
            let updated_at = bson_to_iso(&doc.updated_at);
            let id_hex = match doc.id {
                Some(oid) => oid.to_hex(),
                None => generate_id_hex(),
            };

            sqlx::query(
                "INSERT INTO shop_profiles ( \
                    key, id, legal_name, trading_name, address_lines, primary_phone, secondary_phone, \
                    email, website, business_reg_no, logo_base64, bank_name, bank_branch, account_name, \
                    account_number, default_warranty_text, default_footer_text, receipt_footer_text, \
                    allow_shared_user_sessions, allow_staff_multi_device_sessions, version, created_at, \
                    updated_at, updated_by_device \
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24) \
                ON CONFLICT(key) DO UPDATE SET \
                    legal_name = excluded.legal_name, \
                    trading_name = excluded.trading_name, \
                    address_lines = excluded.address_lines, \
                    primary_phone = excluded.primary_phone, \
                    secondary_phone = excluded.secondary_phone, \
                    email = excluded.email, \
                    website = excluded.website, \
                    business_reg_no = excluded.business_reg_no, \
                    logo_base64 = excluded.logo_base64, \
                    bank_name = excluded.bank_name, \
                    bank_branch = excluded.bank_branch, \
                    account_name = excluded.account_name, \
                    account_number = excluded.account_number, \
                    default_warranty_text = excluded.default_warranty_text, \
                    default_footer_text = excluded.default_footer_text, \
                    receipt_footer_text = excluded.receipt_footer_text, \
                    allow_shared_user_sessions = excluded.allow_shared_user_sessions, \
                    allow_staff_multi_device_sessions = excluded.allow_staff_multi_device_sessions, \
                    version = excluded.version, \
                    updated_at = excluded.updated_at, \
                    updated_by_device = excluded.updated_by_device",
            )
            .bind(&doc.key)
            .bind(&id_hex)
            .bind(&doc.legal_name)
            .bind(&doc.trading_name)
            .bind(&address_lines_json)
            .bind(&doc.primary_phone)
            .bind(&doc.secondary_phone)
            .bind(&doc.email)
            .bind(&doc.website)
            .bind(&doc.business_reg_no)
            .bind(&doc.logo_base64)
            .bind(&doc.bank_name)
            .bind(&doc.bank_branch)
            .bind(&doc.account_name)
            .bind(&doc.account_number)
            .bind(&doc.default_warranty_text)
            .bind(&doc.default_footer_text)
            .bind(&doc.receipt_footer_text)
            .bind(if doc.allow_shared_user_sessions { 1i64 } else { 0i64 })
            .bind(if doc.allow_staff_multi_device_sessions {
                1i64
            } else {
                0i64
            })
            .bind(doc.version)
            .bind(&created_at)
            .bind(&updated_at)
            .bind(&doc.updated_by_device)
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::internal(format!("Failed to save shop profile: {e}")))?;

            Ok(())
        }
        Backend::Mongo(mongo, session) => {
            let collection = mongo.collection::<mongodb::bson::Document>("shop_profiles");
            let address_lines_bson: Vec<Bson> = doc
                .address_lines
                .iter()
                .map(|s| Bson::String(s.clone()))
                .collect();

            let mut set_doc = doc! {
                "key": &doc.key,
                "legal_name": &doc.legal_name,
                "trading_name": &doc.trading_name,
                "address_lines": address_lines_bson,
                "primary_phone": &doc.primary_phone,
                "secondary_phone": &doc.secondary_phone,
                "email": &doc.email,
                "website": &doc.website,
                "business_reg_no": &doc.business_reg_no,
                "logo_base64": &doc.logo_base64,
                "bank_name": &doc.bank_name,
                "bank_branch": &doc.bank_branch,
                "account_name": &doc.account_name,
                "account_number": &doc.account_number,
                "default_warranty_text": &doc.default_warranty_text,
                "default_footer_text": &doc.default_footer_text,
                "receipt_footer_text": &doc.receipt_footer_text,
                "allow_shared_user_sessions": doc.allow_shared_user_sessions,
                "allow_staff_multi_device_sessions": doc.allow_staff_multi_device_sessions,
                "version": doc.version,
                "updated_at": doc.updated_at,
            };
            if let Some(device) = &doc.updated_by_device {
                set_doc.insert("updated_by_device", device);
            }
            let mut on_insert = doc! { "created_at": doc.created_at };
            if let Some(oid) = doc.id {
                on_insert.insert("_id", oid);
            }

            let update_doc = doc! {
                "$set": set_doc,
                "$setOnInsert": on_insert,
            };

            crate::in_session!(
                collection
                    .update_one(doc! { "key": &doc.key }, update_doc)
                    .upsert(true),
                session
            )
            .map_err(|e| AppError::internal(format!("Failed to save shop profile: {e}")))?;

            Ok(())
        }
    }
}
