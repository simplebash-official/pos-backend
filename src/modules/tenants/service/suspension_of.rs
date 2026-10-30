use super::retained::Retained;
use super::suspension_cache::suspension_cache;
use super::suspension_info::SuspensionInfo;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::modules::tenants::model::TenantDocument;
use crate::modules::tenants::{repository, usage};

// ----------------------------------------------------------------------------
// Suspension
// ----------------------------------------------------------------------------

/// How long a looked-up suspension flag is trusted before the next request re-reads it. Changes
/// made through `set_suspended` on this instance take effect at once; another instance of the
/// backend picks them up within this window, which is the "about 30 seconds" the cloud promises.
const SUSPENSION_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// The shop's current suspension, `None` while it is active. Cached for `SUSPENSION_TTL`, so the
/// retained-data counts are computed at most once per window however often a refused client retries.
/// Always `None` outside the multi-tenant (MongoDB) cloud and for a tenant that is not in the
/// directory (its token is then refused elsewhere).
pub(crate) async fn suspension_of(db: &Db, tenant_key: &str) -> AppResult<Option<SuspensionInfo>> {
    if db.as_mongo().is_none() {
        return Ok(None);
    }
    if let Some(known) = suspension_cache()
        .get(tenant_key)
        .filter(|(_, read_at)| read_at.elapsed() < SUSPENSION_TTL)
        .map(|(info, _)| info.clone())
    {
        return Ok(known);
    }
    let info = match repository::find_tenant_by_key(db, tenant_key).await? {
        Some(TenantDocument {
            name,
            suspended_at: Some(at),
            suspended_reason,
            ..
        }) => {
            let reason = suspended_reason.ok_or_else(|| {
                AppError::internal(format!("shop {tenant_key} is suspended without a reason"))
            })?;
            let usage = usage::usage_for(db, tenant_key).await?;
            Some(SuspensionInfo {
                shop_name: name,
                suspended_at: at
                    .try_to_rfc3339_string()
                    .map_err(|e| AppError::internal(e.to_string()))?,
                reason,
                retained: Retained {
                    bills: usage.bills_total,
                    products: usage.products,
                    customers: usage.customers,
                    staff: usage.users,
                    last_bill_at: usage.last_bill_at,
                },
            })
        }
        _ => None,
    };
    suspension_cache().insert(
        tenant_key.to_string(),
        (info.clone(), std::time::Instant::now()),
    );
    Ok(info)
}
