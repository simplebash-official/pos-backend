// One-time MongoDB move of `cash_movements` to typed movements with categories, the
// cloud twin of `clients::sqlite::migrate_cash_movements` (same mapping, same
// deterministic category keys, so a desktop and the cloud agree and sync merges instead of
// duplicating). Run by `cargo run --bin migrate_cash_movements`; idempotent, since only
// documents that still carry the old `reason` field are touched. Also makes sure every
// shop has the built-in categories the migrated movements point at.

use futures_util::TryStreamExt;
use mongodb::{
    Database,
    bson::{Bson, Document, doc},
};

use crate::{
    clients::db::Db,
    core::{
        error::{AppError, AppResult},
        tenancy::Tenant,
    },
    domain::{cash_categories::category_key, sessions::CashMovementType as T},
    modules::cash_categories::service,
};

/// What a run did (or, in a dry run, would do).
#[derive(Debug, Default, Clone, Copy)]
pub struct Report {
    pub shops_checked: usize,
    pub movements: usize,
}

fn mapped(old_type: &str) -> AppResult<(T, &'static str, String)> {
    let other = |t: T| category_key(t, "Other");
    Ok(match old_type {
        "pay_in" => (T::AddCash, "in", other(T::AddCash)),
        "pay_out_expense" => (T::PettyExpense, "out", other(T::PettyExpense)),
        "pay_out_supplier" => (T::SupplierPayment, "out", other(T::SupplierPayment)),
        "owner_withdrawal" => (T::OwnerWithdrawal, "out", other(T::OwnerWithdrawal)),
        "cash_drop" => (T::CashDrop, "neutral", category_key(T::CashDrop, "To safe")),
        "drawer_open_no_sale" => (T::NoSale, "neutral", other(T::NoSale)),
        unknown => {
            return Err(AppError::internal(format!(
                "Cannot migrate a cash movement of unknown type '{unknown}'"
            )));
        }
    })
}

/// Migrates every shop in `db` (all tenants of a multi-tenant cloud, or the single shop).
pub async fn migrate_mongo(db: &Database, multi_tenant: bool, dry_run: bool) -> AppResult<Report> {
    let mut report = Report::default();

    let shops: Vec<Option<String>> = if multi_tenant {
        db.collection::<Document>("tenants")
            .distinct("key", doc! {})
            .await?
            .into_iter()
            .filter_map(|k| match k {
                Bson::String(key) => Some(Some(key)),
                _ => None,
            })
            .collect()
    } else {
        vec![None]
    };
    for shop in shops {
        report.shops_checked += 1;
        if dry_run {
            continue;
        }
        let handle = Db::from_mongo(db.clone());
        match shop {
            Some(tenant) => {
                let scoped = handle.for_tenant(Tenant::id(&tenant)?);
                service::ensure_system_categories(&scoped).await?;
            }
            None => {
                service::ensure_system_categories(&handle).await?;
            }
        }
    }

    let movements = db.collection::<Document>("cash_movements");
    let old: Vec<Document> = movements
        .find(doc! { "reason": { "$exists": true } })
        .await?
        .try_collect()
        .await?;
    for movement in old {
        report.movements += 1;
        if dry_run {
            continue;
        }
        let old_type = movement
            .get_str("movement_type")
            .map_err(|e| AppError::internal(e.to_string()))?;
        let (new_type, direction, category) = mapped(old_type)?;
        let mut set = doc! {
            "movement_type": new_type.as_str(),
            "direction": direction,
            "category_key": category,
            "note": movement.get_str("reason").map_err(|e| AppError::internal(e.to_string()))?,
        };
        if let Ok(who) = movement.get_str("recipient_or_source") {
            set.insert("payee_name", who);
        }
        if let Ok(supplier) = movement.get_str("supplier_key") {
            set.insert("payee_key", supplier);
        }
        let id = movement
            .get_object_id("_id")
            .map_err(|e| AppError::internal(e.to_string()))?;
        movements
            .update_one(
                doc! { "_id": id },
                doc! { "$set": set, "$unset": { "reason": "", "recipient_or_source": "", "supplier_key": "" } },
            )
            .await?;
    }
    Ok(report)
}
