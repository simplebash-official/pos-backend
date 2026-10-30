use crate::clients::db::Db;
use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::modules::settings::model::ShopProfileDocument;
use mongodb::bson::doc;
use sqlx::Row;

pub(crate) async fn find_shop_profile(
    db: &Db,
    key: &str,
) -> AppResult<Option<ShopProfileDocument>> {
    match db {
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT key, id, legal_name, trading_name, address_lines, primary_phone, \
                 secondary_phone, email, website, business_reg_no, logo_base64, bank_name, \
                 bank_branch, account_name, account_number, default_warranty_text, \
                 default_footer_text, receipt_footer_text, allow_shared_user_sessions, version, created_at, updated_at \
                 FROM shop_profiles WHERE key = ?1 LIMIT 1",
            )
            .bind(key)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to fetch shop profile: {e}")))?;

            Ok(row.map(|r| {
                let address_lines_str: String = r.get("address_lines");
                let address_lines: Vec<String> =
                    serde_json::from_str(&address_lines_str).unwrap_or_default();
                let created_at: String = r.get("created_at");
                let updated_at: String = r.get("updated_at");
                let allow_shared_user_sessions_int: i64 =
                    r.try_get("allow_shared_user_sessions").unwrap_or(0);

                ShopProfileDocument {
                    id: None,
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
                    version: r.get("version"),
                    created_at: to_bson_datetime(&created_at),
                    updated_at: to_bson_datetime(&updated_at),
                }
            }))
        }
        Db::Mongo(mongo) => {
            let collection = mongo.collection::<ShopProfileDocument>("shop_profiles");
            let doc_opt = collection
                .find_one(doc! { "key": key })
                .await
                .map_err(|e| AppError::internal(format!("Failed to fetch shop profile: {e}")))?;
            Ok(doc_opt)
        }
    }
}
