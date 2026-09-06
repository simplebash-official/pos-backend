use chrono::Utc;
use rand::{RngExt, distr::Alphanumeric};

use crate::{
    clients::db::Db,
    core::{constants::prefixes, error::AppError, id::generate_id},
};

#[derive(Debug, Clone)]
pub struct ApiKeySeedResult {
    pub key: String,
    pub secret: String,
}

pub async fn seed_api_key(db: &Db) -> Result<ApiKeySeedResult, AppError> {
    let secret: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();
    let key = generate_id(prefixes::API_KEY);
    let now_iso = Utc::now().to_rfc3339();

    match db {
        Db::Mongo(mongo_db) => {
            mongo_db
                .collection::<mongodb::bson::Document>("api_keys")
                .insert_one(mongodb::bson::doc! {
                    "key": &key,
                    "secret": &secret,
                    "name": "Admin API Key",
                    "role": "admin",
                    "created_at": &now_iso,
                    "updated_at": &now_iso,
                })
                .await
                .map_err(|e| {
                    AppError::internal(format!("failed to insert api key in mongo: {e}"))
                })?;
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO api_keys (key, id, hashed_key, name, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&key)
            .bind(&key)
            .bind(&secret)
            .bind("Admin API Key")
            .bind("admin")
            .bind(&now_iso)
            .bind(&now_iso)
            .execute(pool)
            .await
            .map_err(|e| AppError::internal(format!("failed to insert api key in sqlite: {e}")))?;
        }
    }

    Ok(ApiKeySeedResult { key, secret })
}
