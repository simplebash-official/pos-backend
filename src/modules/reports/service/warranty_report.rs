//! The warranty report for a period chosen like every other analytics report (shop-local
//! presets or days). The figures come from the warranties module.

use super::dates;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::AnalyticsRangeQuery;
use crate::domain::warranties::WarrantyReport;
use crate::modules::warranties;

/// The warranty report for the requested period.
pub(crate) async fn get_warranty_report(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<WarrantyReport> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    warranties::service::report(db, start, end).await
}
