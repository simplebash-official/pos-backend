use super::report::Report;
use crate::clients::db::Db;
use crate::core::config::TenantMode;
use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::Tenant;
use crate::domain::cash_categories::category_key;
use crate::domain::sessions::CashMovementType as T;
use crate::modules::cash_categories::service;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};

struct MappedMovement {
    movement_type: T,
    direction: &'static str,
    category: String,
}

fn mapped(old_type: &str) -> AppResult<MappedMovement> {
    let other = |t: T| category_key(t, "Other");
    Ok(match old_type {
        "pay_in" => MappedMovement {
            movement_type: T::AddCash,
            direction: "in",
            category: other(T::AddCash),
        },
        "pay_out_expense" => MappedMovement {
            movement_type: T::PettyExpense,
            direction: "out",
            category: other(T::PettyExpense),
        },
        "pay_out_supplier" => MappedMovement {
            movement_type: T::SupplierPayment,
            direction: "out",
            category: other(T::SupplierPayment),
        },
        "owner_withdrawal" => MappedMovement {
            movement_type: T::OwnerWithdrawal,
            direction: "out",
            category: other(T::OwnerWithdrawal),
        },
        "cash_drop" => MappedMovement {
            movement_type: T::CashDrop,
            direction: "neutral",
            category: category_key(T::CashDrop, "To safe"),
        },
        "drawer_open_no_sale" => MappedMovement {
            movement_type: T::NoSale,
            direction: "neutral",
            category: other(T::NoSale),
        },
        unknown => {
            return Err(AppError::internal(format!(
                "Cannot migrate a cash movement of unknown type '{unknown}'"
            )));
        }
    })
}

/// Migrates every shop in `db` (all tenants of a multi-tenant cloud, or the single shop).
pub async fn migrate_mongo(
    db: &Database,
    tenant_mode: TenantMode,
    mode: Mode,
) -> AppResult<Report> {
    let mut report = Report::default();

    let shops: Vec<Option<String>> = if tenant_mode == TenantMode::Multi {
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
        if mode == Mode::DryRun {
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
        if mode == Mode::DryRun {
            continue;
        }
        let old_type = movement
            .get_str("movement_type")
            .map_err(|e| AppError::internal(e.to_string()))?;
        let MappedMovement {
            movement_type: new_type,
            direction,
            category,
        } = mapped(old_type)?;
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
