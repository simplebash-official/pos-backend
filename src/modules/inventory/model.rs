// Mongo document shapes for the inventory feature. Kept separate from
// `domain::inventory` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::inventory::{CategoryInfo, Product, StockMovement, StockMovementType},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub sku: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub name: String,
    pub category: String,
    pub subcategory: String,
    pub cost_price_cents: i64,
    pub selling_price_cents: i64,
    pub stock_quantity: i64,
    pub min_stock_threshold: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
}

impl ProductDocument {
    pub fn into_product(self) -> Product {
        let key = if self.key.is_empty() {
            generate_id(prefixes::PRODUCT)
        } else {
            self.key
        };
        Product {
            id: self
                .id
                .expect("persisted product document must have an id")
                .to_hex(),
            key,
            sku: self.sku,
            barcode: self.barcode,
            name: self.name,
            category: self.category,
            subcategory: self.subcategory,
            cost_price_cents: self.cost_price_cents,
            selling_price_cents: self.selling_price_cents,
            stock_quantity: self.stock_quantity,
            min_stock_threshold: self.min_stock_threshold,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub product_id: ObjectId,
    pub quantity_delta: i64,
    pub movement_type: StockMovementType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl StockMovementDocument {
    pub fn into_stock_movement(self) -> StockMovement {
        let key = if self.key.is_empty() {
            generate_id(prefixes::STOCK_MOVEMENT)
        } else {
            self.key
        };
        StockMovement {
            id: self
                .id
                .expect("persisted stock movement document must have an id")
                .to_hex(),
            key,
            product_id: self.product_id.to_hex(),
            quantity_delta: self.quantity_delta,
            movement_type: self.movement_type,
            reference_id: self.reference_id,
            note: self.note,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}

/// A main category and its allowed subcategories, persisted in the
/// `categories` collection so the valid set is a single DB-backed source of
/// truth (for both API responses and product validation) instead of a
/// static table baked into the binary. Seeded via `cargo run --bin seed_providers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub subcategories: Vec<String>,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl CategoryDocument {
    pub fn into_category_info(self) -> CategoryInfo {
        let key = if self.key.is_empty() {
            generate_id(prefixes::CATEGORY)
        } else {
            self.key
        };
        CategoryInfo {
            key,
            name: self.name,
            icon: self.icon,
            color: self.color,
            subcategories: self.subcategories,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
