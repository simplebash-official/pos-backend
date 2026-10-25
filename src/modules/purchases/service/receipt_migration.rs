// One-time MongoDB migration that gives every purchase made before supplier receipts a receipt
// to belong to (`cargo run --bin migrate_purchase_receipts`). Purchases that share a shop, a
// supplier, an invoice number and a day become one receipt; a purchase without an invoice
// number is a receipt of its own. The receipt key and number are derived from the smallest
// purchase key in the group, exactly as the SQLite migration derives them
// (`clients::sqlite::migrate_purchases_to_receipts`), so a desktop and the cloud migrating
// separately write identical rows and sync merges them. Idempotent: only purchases without a
// `receipt_key` are touched, and a receipt that already exists is left alone. The same run moves
// every purchase line from a per-unit cost to a line total (`line_total_cents` = quantity x the old
// unit cost, no share of extra charges), which is how a line is priced since extra charges and lot
// prices arrived.

use std::collections::BTreeMap;

use futures_util::TryStreamExt;
use mongodb::{
    Database,
    bson::{Bson, Document, doc},
};

use crate::{
    core::tenancy::TENANT_FIELD, modules::inventory::service::stock_batch_migration::Mode,
};

/// What a run changed (or, in a dry run, would change).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub purchases: u64,
    pub receipts: u64,
    /// Purchase lines moved from a per-unit cost to a line total.
    pub line_totals: u64,
}

fn text(document: &Document, field: &str) -> Option<String> {
    document
        .get_str(field)
        .ok()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// The last `n` characters, as SQLite's `substr(x, -n)` takes them.
fn tail(value: &str, n: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    chars[chars.len().saturating_sub(n)..].iter().collect()
}

/// Migrates every shop in the database.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> mongodb::error::Result<Summary> {
    let purchases = db.collection::<Document>("purchases");
    let receipts = db.collection::<Document>("purchase_receipts");

    let unmigrated: Vec<Document> = purchases
        .find(doc! { "receipt_key": { "$exists": false } })
        .await?
        .try_collect()
        .await?;

    // (shop, supplier, invoice number or own key, day) -> the group's purchases.
    let mut groups: BTreeMap<(String, String, String, String), Vec<&Document>> = BTreeMap::new();
    for purchase in &unmigrated {
        let key = purchase.get_str("key").unwrap_or_default().to_string();
        let shop = purchase
            .get(TENANT_FIELD)
            .map(|t| t.to_string())
            .unwrap_or_default();
        let supplier = purchase
            .get_str("supplier_key")
            .unwrap_or_default()
            .to_string();
        let invoice = text(purchase, "reference_no").unwrap_or_else(|| key.clone());
        let date = purchase
            .get_datetime("date")
            .map(|d| d.timestamp_millis().to_string())
            .unwrap_or_default();
        groups
            .entry((shop, supplier, invoice, date))
            .or_default()
            .push(purchase);
    }

    let mut summary = Summary::default();
    for members in groups.values() {
        let first = members
            .iter()
            .filter_map(|p| p.get_str("key").ok())
            .min()
            .unwrap_or_default()
            .to_string();
        let representative = members[0];
        let receipt_key = format!("prc_leg_{first}");
        let created_at = members
            .iter()
            .filter_map(|p| p.get_datetime("created_at").ok().copied())
            .min();

        let mut receipt = doc! {
            "key": &receipt_key,
            "receipt_number": format!("PUR-L{}", tail(&first, 10)),
            "supplier_key": representative.get_str("supplier_key").unwrap_or_default(),
            "date": representative.get("date").cloned().unwrap_or(Bson::Null),
            "version": 1_i64,
        };
        if let Some(reference) = members.iter().find_map(|p| text(p, "reference_no")) {
            receipt.insert("reference_no", reference);
        }
        if let Some(notes) = members.iter().find_map(|p| text(p, "notes")) {
            receipt.insert("notes", notes);
        }
        if let Some(created_at) = created_at {
            receipt.insert("created_at", created_at);
            receipt.insert("updated_at", created_at);
        }
        if let Some(shop) = representative.get(TENANT_FIELD) {
            receipt.insert(TENANT_FIELD, shop.clone());
        }

        summary.purchases += members.len() as u64;
        summary.receipts += 1;
        if mode == Mode::DryRun {
            continue;
        }

        // `$setOnInsert` makes a re-run (or a receipt another device already synced) a no-op.
        let mut filter = doc! { "key": &receipt_key };
        if let Some(shop) = representative.get(TENANT_FIELD) {
            filter.insert(TENANT_FIELD, shop.clone());
        }
        receipts
            .update_one(filter, doc! { "$setOnInsert": receipt })
            .upsert(true)
            .await?;
        let ids: Vec<Bson> = members
            .iter()
            .filter_map(|p| p.get("_id").cloned())
            .collect();
        purchases
            .update_many(
                doc! { "_id": { "$in": ids } },
                doc! {
                    "$set": { "receipt_key": &receipt_key },
                    "$unset": { "reference_no": "", "notes": "" },
                },
            )
            .await?;
    }

    // Lines from before lot prices carry a per-unit cost; the line total is its product with the
    // quantity. One update computes it on the server, so every line is migrated the same way.
    let pending = doc! { "line_total_cents": { "$exists": false } };
    summary.line_totals = purchases.count_documents(pending.clone()).await?;
    if summary.line_totals > 0 && mode == Mode::Apply {
        purchases
            .update_many(
                pending,
                vec![
                    doc! { "$set": {
                        "line_total_cents": { "$multiply": ["$quantity", "$unit_cost_cents"] },
                        "charges_share_cents": 0_i64,
                    } },
                    doc! { "$unset": "unit_cost_cents" },
                ],
            )
            .await?;
    }
    Ok(summary)
}
