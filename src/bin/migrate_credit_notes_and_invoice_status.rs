// One-time data migration for the Invoice Status Model + Credit Note
// upgrade (see /Users/mayura/.claude/plans/now-here-is-the-enchanted-storm.md).
// Rewrites pre-migration data left over from the old `"cancelled"` invoice
// status / `returns` collection / `restock_action` field shape into the new
// `InvoiceStatus::Voided` / `credit_notes` collection / `condition`+
// `disposition` shape the new backend code assumes with no fallback.
//
// Idempotent and safe to re-run: every step checks the document is still in
// the OLD shape before rewriting it, so running this against a database
// that's already partially or fully migrated (or has no legacy data at all)
// is a no-op for whatever it finds already-migrated.
//
// Run with: `cargo run --bin migrate_credit_notes_and_invoice_status`
// MUST run before deploying the new backend binary in any environment that
// has pre-existing data — the new code has no compat shim for the old shape.

use mongodb::{
    Database,
    bson::{Document, doc},
};
use simplebash_pos_backend::{
    clients, core::config::Config, core::constants::prefixes, core::id::generate_id,
};
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env().expect("invalid configuration");
    let db = clients::mongo::connect(&config.mongodb_uri, &config.mongodb_db_name)
        .await
        .expect("failed to connect to MongoDB");

    println!("== Step 1: invoices status/field rename ==");
    let step1 = migrate_invoices(&db).await;
    println!(
        "  rewrote status cancelled->voided on {} document(s); renamed cancelled_*->voided_* on {} document(s)",
        step1.0, step1.1
    );

    println!("== Step 2: returns -> credit_notes collection + shape rewrite ==");
    let key_map = migrate_returns_to_credit_notes(&db).await;
    println!(
        "  migrated {} return document(s) into credit_notes",
        key_map.len()
    );

    println!("== Step 2b: repair credit_notes mangled by an earlier buggy run ==");
    let step2b = repair_mismigrated_credit_notes(&db).await;
    println!("  repaired {step2b} document(s)");

    println!("== Step 3: backfill invoices.creditNoteCount ==");
    let step3 = backfill_credit_note_counts(&db).await;
    println!("  updated creditNoteCount on {step3} invoice(s)");

    println!("== Step 4: stock_movements 'return' reclassification ==");
    let (restock, void_reversal) = reclassify_stock_movements(&db, &key_map).await;
    println!(
        "  reclassified {restock} as return_restock (matched a migrated credit note), {void_reversal} as invoice_void_reversal (everything else — the documented fallback heuristic)"
    );

    println!("== Done ==");
}

/// Step 1: `status:"cancelled"` -> `"voided"`, and `cancelled_at/by/reason`
/// -> `voided_at/by/reason` (only where the old fields are still present —
/// a document already in the new shape has nothing to rename and is left
/// untouched). `credit_note_count` is backfilled as 0 here (a placeholder —
/// step 3 computes the real value from the migrated `credit_notes`
/// collection, which doesn't exist yet at this point in the script).
async fn migrate_invoices(db: &Database) -> (u64, u64) {
    let invoices = db.collection::<Document>("invoices");

    let status_result = invoices
        .update_many(
            doc! { "status": "cancelled" },
            doc! { "$set": { "status": "voided" } },
        )
        .await
        .expect("failed to rewrite invoice status cancelled->voided");

    let rename_result = invoices
        .update_many(
            doc! { "cancelled_at": { "$exists": true } },
            doc! {
                "$rename": {
                    "cancelled_at": "voided_at",
                    "cancelled_by": "voided_by",
                    "cancellation_reason": "voided_reason",
                },
            },
        )
        .await
        .expect("failed to rename cancelled_*->voided_* fields");

    invoices
        .update_many(
            doc! { "credit_note_count": { "$exists": false } },
            doc! { "$set": { "credit_note_count": 0i64 } },
        )
        .await
        .expect("failed to backfill credit_note_count placeholder");

    (status_result.modified_count, rename_result.modified_count)
}

/// Step 2: merges legacy documents out of the `returns` collection into
/// `credit_notes`, transforming each one's `restock_action` into
/// `condition`/`disposition` and regenerating its `ret_...` key as a
/// `cn_...` key (the prefix itself changed, not just the collection). This
/// is a per-document copy, **not** a collection rename: by the time this
/// migration runs against a real environment, `credit_notes` already exists
/// and already holds genuine post-migration documents (the new backend code
/// writes straight into it), so `renameCollection` — which refuses to
/// overwrite an existing target namespace — is the wrong tool here even
/// setting aside that MongoDB Atlas also rejects it unless issued against
/// the `admin` database. A no-op if `returns` doesn't exist or is already
/// empty (never had the old Returns feature, or already migrated). Returns
/// the old-key -> new-key map, needed by step 4 to reclassify stock
/// movements that reference the old keys.
async fn migrate_returns_to_credit_notes(db: &Database) -> HashMap<String, String> {
    let collection_names = db
        .list_collection_names()
        .await
        .expect("failed to list collections");

    if !collection_names.iter().any(|name| name == "returns") {
        println!("  no 'returns' collection found, nothing to migrate");
        return HashMap::new();
    }

    let returns = db.collection::<Document>("returns");
    let credit_notes = db.collection::<Document>("credit_notes");
    let mut key_map = HashMap::new();

    use futures_util::TryStreamExt;
    let mut cursor = returns
        .find(doc! {})
        .await
        .expect("failed to scan legacy 'returns' collection");
    let mut legacy_docs = Vec::new();
    while let Some(doc) = cursor
        .try_next()
        .await
        .expect("failed to read legacy return document")
    {
        legacy_docs.push(doc);
    }

    if legacy_docs.is_empty() {
        println!("  'returns' collection exists but is empty, dropping it");
        returns
            .drop()
            .await
            .expect("failed to drop empty 'returns' collection");
        return key_map;
    }

    for mut document in legacy_docs {
        document.remove("_id"); // let Mongo assign a fresh _id in credit_notes

        let old_key = document.get_str("key").unwrap_or_default().to_string();
        let new_key = generate_id(prefixes::CREDIT_NOTE);
        if !old_key.is_empty() {
            key_map.insert(old_key, new_key.clone());
        }

        // `restock_action` (and the new `condition`/`disposition` that
        // replace it) live on each entry of `returned_items`, not on the
        // document itself — the old `ReturnItem` shape had this per line,
        // matching the new `CreditNoteItem` shape exactly at that level.
        if let Ok(returned_items) = document.get_array_mut("returned_items") {
            for item in returned_items.iter_mut() {
                let mongodb::bson::Bson::Document(item_doc) = item else {
                    continue;
                };
                let restock_action = item_doc
                    .get_str("restock_action")
                    .unwrap_or_default()
                    .to_string();
                let (condition, disposition): (String, Option<String>) =
                    match restock_action.as_str() {
                        "restock_to_inventory" => ("resalable".to_string(), None),
                        // Old data has no disposition concept — a
                        // deliberate, lossy-but-safe default for
                        // pre-migration damaged returns (documented in the
                        // plan): treat as damaged + write-off.
                        "damaged_discard" => {
                            ("damaged".to_string(), Some("write_off_scrap".to_string()))
                        }
                        // No restock_action at all (e.g. an already-partially
                        // migrated or otherwise malformed legacy item) — fall
                        // back to the safest assumption, matching a fresh
                        // credit note's own default.
                        _ => ("resalable".to_string(), None),
                    };
                item_doc.remove("restock_action");
                item_doc.insert("condition", condition);
                if let Some(disposition) = disposition {
                    item_doc.insert("disposition", disposition);
                }
            }
        }

        document.remove("restock_action");
        document.insert("key", new_key);
        // `credit_note_number` is the new name for the old `return_number`
        // field — required (no `#[serde(default)]`), so a document still
        // carrying the old name fails deserialization entirely.
        if let Ok(return_number) = document.get_str("return_number") {
            let return_number = return_number.to_string();
            document.remove("return_number");
            document.insert("credit_note_number", return_number);
        }
        document
            .entry("no_receipt".to_string())
            .or_insert(mongodb::bson::Bson::Boolean(false));
        document
            .entry("is_manager_override".to_string())
            .or_insert(mongodb::bson::Bson::Boolean(false));
        document
            .entry("status".to_string())
            .or_insert(mongodb::bson::Bson::String("resolved".to_string()));

        credit_notes
            .insert_one(document)
            .await
            .expect("failed to insert migrated credit note document");
    }

    println!("  copied {} document(s) into 'credit_notes'", key_map.len());
    returns
        .drop()
        .await
        .expect("failed to drop migrated 'returns' collection");
    println!("  dropped 'returns' collection");

    key_map
}

/// Step 2b: one-time repair for documents mangled by an earlier, buggy
/// version of this script's step 2, which (a) set a bogus top-level
/// `condition` field on the whole `credit_notes` document instead of on
/// each entry of `returned_items` (where the real `CreditNoteItem.condition`
/// field actually lives), and (b) left the old `return_number` field
/// unrenamed instead of `credit_note_number` (a required field — its
/// absence alone is enough to fail deserialization even once (a) is fixed).
/// Matches any document showing either symptom; each fix internally checks
/// whether it's still needed, so re-running against an already-repaired
/// document (or a never-mangled one that happens to match neither
/// condition) is a safe no-op either way.
async fn repair_mismigrated_credit_notes(db: &Database) -> u64 {
    let credit_notes = db.collection::<Document>("credit_notes");

    use futures_util::TryStreamExt;
    let mut cursor = credit_notes
        .find(doc! { "$or": [
            { "condition": { "$exists": true } },
            { "return_number": { "$exists": true } },
            { "credit_note_number": { "$exists": false } },
        ] })
        .await
        .expect("failed to scan mangled credit note documents");
    let mut mangled_docs = Vec::new();
    while let Some(doc) = cursor
        .try_next()
        .await
        .expect("failed to read mangled credit note document")
    {
        mangled_docs.push(doc);
    }

    let mut repaired = 0u64;
    for mut document in mangled_docs {
        let Some(id) = document.get_object_id("_id").ok() else {
            continue;
        };
        document.remove("condition");
        document.remove("disposition");

        if let Ok(return_number) = document.get_str("return_number") {
            let return_number = return_number.to_string();
            document.remove("return_number");
            document
                .entry("credit_note_number".to_string())
                .or_insert(mongodb::bson::Bson::String(return_number));
        }

        if let Ok(returned_items) = document.get_array_mut("returned_items") {
            for item in returned_items.iter_mut() {
                let mongodb::bson::Bson::Document(item_doc) = item else {
                    continue;
                };
                if item_doc.contains_key("condition") {
                    continue; // already correct, leave as-is
                }
                let restock_action = item_doc
                    .get_str("restock_action")
                    .unwrap_or_default()
                    .to_string();
                let (condition, disposition): (String, Option<String>) =
                    match restock_action.as_str() {
                        "restock_to_inventory" => ("resalable".to_string(), None),
                        "damaged_discard" => {
                            ("damaged".to_string(), Some("write_off_scrap".to_string()))
                        }
                        _ => ("resalable".to_string(), None),
                    };
                item_doc.remove("restock_action");
                item_doc.insert("condition", condition);
                if let Some(disposition) = disposition {
                    item_doc.insert("disposition", disposition);
                }
            }
        }

        credit_notes
            .replace_one(doc! { "_id": id }, document)
            .await
            .expect("failed to repair mangled credit note document");
        repaired += 1;
    }

    repaired
}

/// Step 3: recomputes `invoices.creditNoteCount` from an aggregate over the
/// (now-migrated) `credit_notes` collection, grouped by `invoice_key`.
async fn backfill_credit_note_counts(db: &Database) -> u64 {
    let credit_notes = db.collection::<Document>("credit_notes");
    let mut cursor = credit_notes
        .aggregate(vec![
            doc! { "$match": { "invoice_key": { "$exists": true, "$ne": null } } },
            doc! { "$group": { "_id": "$invoice_key", "count": { "$sum": 1 } } },
        ])
        .await
        .expect("failed to aggregate credit note counts");

    use futures_util::TryStreamExt;
    let invoices = db.collection::<Document>("invoices");
    let mut updated = 0u64;
    while let Some(row) = cursor
        .try_next()
        .await
        .expect("failed to read credit note count aggregate row")
    {
        let Some(invoice_key) = row.get_str("_id").ok() else {
            continue;
        };
        let count = row.get_i32("count").unwrap_or(0) as i64;
        let result = invoices
            .update_one(
                doc! { "key": invoice_key },
                doc! { "$set": { "credit_note_count": count } },
            )
            .await
            .expect("failed to backfill invoice credit_note_count");
        updated += result.modified_count;
    }
    updated
}

/// Step 4: rewrites `stock_movements` documents with the old dual-purpose
/// `movement_type: "return"` value. Old data can't distinguish which of the
/// two cases it was — a credit-note-driven restock or an invoice-void
/// stock reversal — since both used the same `Return` variant historically.
/// Heuristic (documented, not perfect): a movement whose `reference_id`
/// matches a migrated credit note's *old* key is reclassified as
/// `return_restock`; everything else defaults to `invoice_void_reversal`
/// (the more common historical case, since `cancel_invoice` — today's
/// `void_invoice` — was the far more frequently exercised path). Returns
/// (restock count, void-reversal count).
async fn reclassify_stock_movements(
    db: &Database,
    key_map: &HashMap<String, String>,
) -> (u64, u64) {
    let stock_movements = db.collection::<Document>("stock_movements");

    let old_keys: Vec<&String> = key_map.keys().collect();
    let restock_result = if old_keys.is_empty() {
        None
    } else {
        Some(
            stock_movements
                .update_many(
                    doc! { "movement_type": "return", "reference_id": { "$in": &old_keys } },
                    doc! { "$set": { "movement_type": "return_restock" } },
                )
                .await
                .expect("failed to reclassify return_restock movements"),
        )
    };
    let restock_count = restock_result.map(|r| r.modified_count).unwrap_or(0);

    let void_reversal_result = stock_movements
        .update_many(
            doc! { "movement_type": "return" },
            doc! { "$set": { "movement_type": "invoice_void_reversal" } },
        )
        .await
        .expect("failed to reclassify invoice_void_reversal movements");

    (restock_count, void_reversal_result.modified_count)
}
