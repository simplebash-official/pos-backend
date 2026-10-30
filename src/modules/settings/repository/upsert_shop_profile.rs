use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::{AppError, AppResult};
use crate::modules::settings::model::ShopProfileDocument;
use mongodb::bson::{Bson, doc};

pub(crate) async fn upsert_shop_profile(db: &Db, doc: &ShopProfileDocument) -> AppResult<()> {
    match db {
        Db::Sqlite(pool) => {
            let address_lines_json =
                serde_json::to_string(&doc.address_lines).unwrap_or_else(|_| "[]".to_string());
            let created_at = bson_to_iso(&doc.created_at);
            let updated_at = bson_to_iso(&doc.updated_at);

            sqlx::query(
                "INSERT INTO shop_profiles ( \
                    key, id, legal_name, trading_name, address_lines, primary_phone, secondary_phone, \
                    email, website, business_reg_no, logo_base64, bank_name, bank_branch, account_name, \
                    account_number, default_warranty_text, default_footer_text, receipt_footer_text, \
                    allow_shared_user_sessions, version, created_at, updated_at \
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22) \
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
                    version = excluded.version, \
                    updated_at = excluded.updated_at",
            )
            .bind(&doc.key)
            .bind(&doc.key)
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
            .bind(doc.version)
            .bind(&created_at)
            .bind(&updated_at)
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to save shop profile: {e}")))?;

            Ok(())
        }
        Db::Mongo(mongo) => {
            let collection = mongo.collection::<mongodb::bson::Document>("shop_profiles");
            let address_lines_bson: Vec<Bson> = doc
                .address_lines
                .iter()
                .map(|s| Bson::String(s.clone()))
                .collect();

            let update_doc = doc! {
                "$set": {
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
                    "version": doc.version,
                    "updated_at": doc.updated_at,
                },
                "$setOnInsert": {
                    "created_at": doc.created_at,
                }
            };

            collection
                .update_one(doc! { "key": &doc.key }, update_doc)
                .upsert(true)
                .await
                .map_err(|e| AppError::internal(format!("Failed to save shop profile: {e}")))?;

            Ok(())
        }
    }
}
