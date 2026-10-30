use super::coverage_status::coverage_status;
use super::last_labels::last_labels;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::core::utils::calculate_pagination;
use crate::domain::inventory::{PaginationMeta, Product, ProductListQuery};
use crate::domain::labels::{
    LabelCoverageCounts, LabelCoverageQuery, LabelCoverageResponse, LabelCoverageRow,
    LabelCoverageStatus,
};
use crate::modules::inventory;

use crate::modules::labels::repository;

use std::collections::HashMap;

/// Products read per page while building the coverage view.
const COVERAGE_SCAN_PAGE: u64 = 200;

/// Most products one `productKeys` coverage request may name (a job's item cap).
const MAX_COVERAGE_KEYS: usize = 200;

fn status_rank(status: Option<LabelCoverageStatus>) -> u8 {
    match status {
        Some(LabelCoverageStatus::Never) => 0,
        Some(LabelCoverageStatus::Needed) => 1,
        Some(LabelCoverageStatus::Labelled) => 2,
        None => 3,
    }
}

/// Every product matching `search` (all of them when `None`), read page by page.
async fn whole_catalogue(db: &Db, search: Option<String>) -> AppResult<Vec<Product>> {
    let mut products = Vec::new();
    let mut page = 1;
    loop {
        let batch = inventory::service::product::list_products(
            db,
            ProductListQuery {
                search: search.clone(),
                page: Some(page),
                limit: Some(COVERAGE_SCAN_PAGE),
                ..Default::default()
            },
        )
        .await?;
        products.extend(batch.items);
        if page >= batch.pagination.total_pages {
            return Ok(products);
        }
        page += 1;
    }
}

/// Every product (optionally matching `search`) with its label state, counts per
/// state, and the requested page of the requested state.
// shortcut: reads the whole catalogue page by page and filters in memory; fine
// for a shop's few thousand products, move into one aggregate if that grows.
pub(crate) async fn coverage(
    db: &Db,
    query: LabelCoverageQuery,
) -> AppResult<LabelCoverageResponse> {
    let search = query.search.filter(|s| !s.trim().is_empty());
    let wanted_keys: Option<Vec<String>> = query.product_keys.map(|keys| {
        keys.split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
            .collect()
    });
    let products = match wanted_keys {
        Some(keys) if keys.len() > MAX_COVERAGE_KEYS => {
            return Err(AppError::validation(format!(
                "Ask for at most {MAX_COVERAGE_KEYS} products at a time."
            )));
        }
        Some(keys) => inventory::service::product::get_products_by_keys(db, &keys).await?,
        None => whole_catalogue(db, search).await?,
    };

    let last = last_labels(repository::list_labelled_entries(db).await?);
    let keys: Vec<String> = products.iter().map(|p| p.key.clone()).collect();
    let mut received_since: HashMap<String, i64> = HashMap::new();
    for units in inventory::service::stock_batch::in_stock_units(db, &keys).await? {
        if let Some(label) = last.get(&units.product_key)
            && units.created_at > label.at
        {
            *received_since.entry(units.product_key).or_default() += units.remaining_quantity;
        }
    }

    let mut rows: Vec<LabelCoverageRow> = products
        .into_iter()
        .map(|product| {
            let label = last.get(&product.key).copied();
            let since = received_since.get(&product.key).copied().unwrap_or(0);
            LabelCoverageRow {
                status: coverage_status(product.stock_quantity, label, since),
                last_labelled_at: label.map(|l| l.at),
                last_labelled_copies: label.map(|l| l.copies),
                received_since_label: since,
                product,
            }
        })
        .collect();
    let count = |status| rows.iter().filter(|r| r.status == Some(status)).count() as u64;
    let counts = LabelCoverageCounts {
        never: count(LabelCoverageStatus::Never),
        needed: count(LabelCoverageStatus::Needed),
        labelled: count(LabelCoverageStatus::Labelled),
    };
    if let Some(wanted) = query.status {
        rows.retain(|row| row.status == Some(wanted));
    }
    rows.sort_by(|a, b| {
        status_rank(a.status)
            .cmp(&status_rank(b.status))
            .then_with(|| {
                a.product
                    .name
                    .to_lowercase()
                    .cmp(&b.product.name.to_lowercase())
            })
    });

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 100);
    let total = rows.len() as u64;
    Ok(LabelCoverageResponse {
        items: rows
            .into_iter()
            .skip(skip as usize)
            .take(limit as usize)
            .collect(),
        pagination: PaginationMeta {
            page,
            limit,
            total,
            total_pages: total.div_ceil(limit),
        },
        counts,
    })
}
