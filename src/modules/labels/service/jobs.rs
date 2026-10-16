// Label print history. A real print or download goes through `create_job`,
// which renders exactly like `render_labels` and then records what was printed,
// how, and by whom; the computer reports back whether its printer step worked
// (`report_job`). `coverage` turns that history plus the stock on hand into a
// per-product "labelled / never labelled / labels needed" view. Previews are
// never recorded.

use std::collections::HashMap;

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::{db::Db, document_server::DocumentServerClient},
    core::{
        constants::prefixes,
        error::{AppError, AppResult},
        id::generate_id,
        utils::calculate_pagination,
    },
    domain::{
        inventory::{PaginationMeta, Product, ProductListQuery},
        labels::{
            CreateLabelJobRequest, LabelCoverageCounts, LabelCoverageQuery, LabelCoverageResponse,
            LabelCoverageRow, LabelCoverageStatus, LabelJob, LabelJobListQuery,
            LabelJobListResponse, LabelJobOutcome, LabelPrintAction, LabelPrintMethod,
            UpdateLabelJobRequest,
        },
    },
    modules::{
        inventory,
        labels::{
            model::{LabelJobItemDocument, LabelPrintJobDocument},
            repository,
        },
    },
};

const JOB_NOT_FOUND: &str = "LABEL_JOB_NOT_FOUND";
const JOB_ALREADY_REPORTED: &str = "LABEL_JOB_ALREADY_REPORTED";
/// The printer resolutions the print setup offers.
const DPI_OPTIONS: [u32; 3] = [203, 300, 600];
const MAX_PRINTER_NAME_CHARS: usize = 200;
const MAX_ERROR_CHARS: usize = 500;
/// Products read per page while building the coverage view.
const COVERAGE_SCAN_PAGE: u64 = 200;
/// Most products one `productKeys` coverage request may name (a job's item cap).
const MAX_COVERAGE_KEYS: usize = 200;

fn job_not_found() -> AppError {
    AppError::not_found_with_code("Label print not found", JOB_NOT_FOUND)
}

/// A print names how it leaves the computer; a download names nothing.
fn validate_job_request(request: &CreateLabelJobRequest) -> AppResult<()> {
    match request.action {
        LabelPrintAction::Download => {
            if request.method.is_some() || request.printer_name.is_some() || request.dpi.is_some() {
                return Err(AppError::validation(
                    "A download has no printer, print method or resolution.",
                ));
            }
        }
        LabelPrintAction::Print => {
            let Some(method) = request.method else {
                return Err(AppError::validation("Choose how the labels print."));
            };
            match request.dpi {
                Some(dpi) if DPI_OPTIONS.contains(&dpi) => {}
                _ => {
                    return Err(AppError::validation(
                        "Choose the printer's resolution: 203, 300 or 600 dpi.",
                    ));
                }
            }
            let printer = request.printer_name.as_deref().map(str::trim);
            match (method, printer) {
                (LabelPrintMethod::Direct, Some(name))
                    if !name.is_empty() && name.chars().count() <= MAX_PRINTER_NAME_CHARS => {}
                (LabelPrintMethod::Direct, _) => {
                    return Err(AppError::validation("Choose the printer to print on."));
                }
                (LabelPrintMethod::System, None) => {}
                (LabelPrintMethod::System, Some(_)) => {
                    return Err(AppError::validation(
                        "The system print window chooses its own printer.",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Renders the chosen items like `render_labels` and records the job. Nothing is
/// recorded when the items cannot be printed. Returns the job and the PDF.
pub(crate) async fn create_job(
    db: &Db,
    doc_server: &DocumentServerClient,
    tenant_id: Option<&str>,
    printed_by_user_id: &str,
    printed_by_name: &str,
    request: CreateLabelJobRequest,
) -> AppResult<(LabelJob, Vec<u8>)> {
    validate_job_request(&request)?;
    let template = super::find_template(db, &request.template_key).await?;
    let stickers = super::resolve_stickers(
        db,
        super::has_price_element(&template.elements),
        &request.items,
    )
    .await?;
    let pdf = super::render(
        db,
        doc_server,
        tenant_id,
        template.width_mm,
        template.height_mm,
        &template.elements,
        &stickers,
    )
    .await?;

    let job = crate::core::logging::domain::tracked("labels.job_created", async {
        let now = BsonDateTime::now();
        let document = LabelPrintJobDocument {
            id: None,
            key: generate_id(prefixes::LABEL_PRINT_JOB),
            template_key: template.key.clone(),
            template_name: template.name.clone(),
            width_mm: template.width_mm,
            height_mm: template.height_mm,
            action: request.action,
            method: request.method,
            printer_name: request
                .printer_name
                .as_deref()
                .map(|n| n.trim().to_string()),
            dpi: request.dpi,
            // A downloaded file is handed over at once; a print waits for the computer's report.
            outcome: match request.action {
                LabelPrintAction::Download => LabelJobOutcome::Sent,
                LabelPrintAction::Print => LabelJobOutcome::Rendered,
            },
            error: None,
            printed_by_user_id: printed_by_user_id.to_string(),
            printed_by_name: printed_by_name.to_string(),
            items: stickers
                .iter()
                .map(|sticker| LabelJobItemDocument {
                    product_key: sticker.product_key.clone(),
                    product_name: sticker.name.clone(),
                    sku: sticker.sku.clone(),
                    serial_number: sticker.serial_number.clone(),
                    price_cents: sticker.price_cents,
                    copies: sticker.copies,
                })
                .collect(),
            total_labels: stickers.iter().map(|sticker| sticker.copies).sum(),
            created_at: now,
            updated_at: now,
        };
        repository::insert_job(db, &document).await?;
        Ok::<_, AppError>(document.into_job())
    })
    .await?;
    Ok((job, pdf))
}

/// Records what the printer step did with a job that is still waiting
/// (`rendered`): `sent` or `failed` with the reason.
pub(crate) async fn report_job(
    db: &Db,
    key: &str,
    request: UpdateLabelJobRequest,
) -> AppResult<LabelJob> {
    crate::core::logging::domain::tracked("labels.job_reported", async move {
        let error = request.error.as_deref().map(str::trim);
        match (request.outcome, error) {
            (LabelJobOutcome::Rendered, _) => {
                return Err(AppError::validation(
                    "Report whether the labels were sent or failed.",
                ));
            }
            (LabelJobOutcome::Sent, Some(_)) => {
                return Err(AppError::validation(
                    "Only a failed print carries a reason.",
                ));
            }
            (LabelJobOutcome::Failed, Some(reason))
                if reason.is_empty() || reason.chars().count() > MAX_ERROR_CHARS =>
            {
                return Err(AppError::validation(
                    "The reason must be 1 to 500 characters.",
                ));
            }
            _ => {}
        }
        if let Some(job) = repository::report_job_outcome(db, key, request.outcome, error).await? {
            return Ok(job.into_job());
        }
        match repository::find_job_by_key(db, key).await? {
            None => Err(job_not_found()),
            Some(_) => Err(AppError::custom(
                StatusCode::CONFLICT,
                JOB_ALREADY_REPORTED,
                "This print has already been reported.",
            )),
        }
    })
    .await
}

/// One recorded print.
pub(crate) async fn get_job(db: &Db, key: &str) -> AppResult<LabelJob> {
    repository::find_job_by_key(db, key)
        .await?
        .map(LabelPrintJobDocument::into_job)
        .ok_or_else(job_not_found)
}

fn parse_instant(value: Option<&str>, field: &str) -> AppResult<Option<BsonDateTime>> {
    value
        .filter(|v| !v.is_empty())
        .map(|v| {
            DateTime::parse_from_rfc3339(v)
                .map(|at| BsonDateTime::from_chrono(at.with_timezone(&Utc)))
                .map_err(|_| AppError::validation(format!("'{field}' must be a date and time.")))
        })
        .transpose()
}

/// A page of the print history, newest first.
pub(crate) async fn list_jobs(
    db: &Db,
    query: LabelJobListQuery,
) -> AppResult<LabelJobListResponse> {
    let filter = repository::JobFilter {
        from: parse_instant(query.from.as_deref(), "from")?,
        to: parse_instant(query.to.as_deref(), "to")?,
        product_key: query.product_key.filter(|k| !k.is_empty()),
        action: query.action,
    };
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 100);
    let (jobs, total) = repository::list_jobs(db, &filter, skip, limit).await?;
    Ok(LabelJobListResponse {
        items: jobs
            .into_iter()
            .map(LabelPrintJobDocument::into_job)
            .collect(),
        pagination: PaginationMeta {
            page,
            limit,
            total,
            total_pages: total.div_ceil(limit),
        },
    })
}

/// A product's latest label: when, and how many stickers it got in that print.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LastLabel {
    at: DateTime<Utc>,
    copies: u32,
}

/// The latest label per product from history entries ordered newest job first.
/// Copies of every group of that product in that one job are added up (a serial
/// product prints one group per unit).
fn last_labels(entries: Vec<repository::LabelledEntry>) -> HashMap<String, LastLabel> {
    let mut latest: HashMap<String, (String, LastLabel)> = HashMap::new();
    for entry in entries {
        match latest.get_mut(&entry.product_key) {
            Some((job_key, last)) if *job_key == entry.job_key => last.copies += entry.copies,
            Some(_) => {}
            None => {
                latest.insert(
                    entry.product_key,
                    (
                        entry.job_key,
                        LastLabel {
                            at: entry.created_at.to_chrono(),
                            copies: entry.copies,
                        },
                    ),
                );
            }
        }
    }
    latest
        .into_iter()
        .map(|(product, (_, last))| (product, last))
        .collect()
}

/// A product's label state: `None` when it was never labelled and has nothing
/// in stock. `received_since` counts units entered after the last label.
fn coverage_status(
    stock_quantity: i64,
    last: Option<LastLabel>,
    received_since: i64,
) -> Option<LabelCoverageStatus> {
    match last {
        None if stock_quantity > 0 => Some(LabelCoverageStatus::Never),
        None => None,
        Some(_) if received_since > 0 => Some(LabelCoverageStatus::Needed),
        Some(_) => Some(LabelCoverageStatus::Labelled),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        action: LabelPrintAction,
        method: Option<LabelPrintMethod>,
        printer: Option<&str>,
        dpi: Option<u32>,
    ) -> CreateLabelJobRequest {
        CreateLabelJobRequest {
            template_key: "starter_x".into(),
            items: Vec::new(),
            action,
            method,
            printer_name: printer.map(str::to_string),
            dpi,
        }
    }

    #[test]
    fn a_job_request_names_exactly_what_its_action_needs() {
        use LabelPrintAction::{Download, Print};
        use LabelPrintMethod::{Direct, System};
        let ok = [
            request(Download, None, None, None),
            request(Print, Some(System), None, Some(203)),
            request(Print, Some(Direct), Some("Zebra ZD220"), Some(300)),
        ];
        for r in ok {
            assert!(validate_job_request(&r).is_ok(), "{r:?}");
        }
        let bad = [
            request(Download, Some(System), None, None),
            request(Download, None, None, Some(203)),
            request(Print, None, None, Some(203)),
            request(Print, Some(System), None, None),
            request(Print, Some(System), None, Some(150)),
            request(Print, Some(System), Some("Zebra"), Some(203)),
            request(Print, Some(Direct), None, Some(203)),
            request(Print, Some(Direct), Some("   "), Some(203)),
            request(Print, Some(Direct), Some(&"x".repeat(201)), Some(203)),
        ];
        for r in bad {
            assert!(validate_job_request(&r).is_err(), "{r:?}");
        }
    }

    fn entry(job: &str, product: &str, copies: u32, at: i64) -> repository::LabelledEntry {
        repository::LabelledEntry {
            job_key: job.into(),
            product_key: product.into(),
            copies,
            created_at: BsonDateTime::from_millis(at),
        }
    }

    #[test]
    fn the_last_label_is_the_newest_job_with_its_groups_added_up() {
        // Newest job first, as the repository returns them.
        let last = last_labels(vec![
            entry("lpj_2", "prod_a", 1, 2_000),
            entry("lpj_2", "prod_a", 1, 2_000),
            entry("lpj_2", "prod_b", 4, 2_000),
            entry("lpj_1", "prod_a", 9, 1_000),
            entry("lpj_1", "prod_c", 2, 1_000),
        ]);
        assert_eq!(last["prod_a"].copies, 2);
        assert_eq!(last["prod_a"].at.timestamp_millis(), 2_000);
        assert_eq!(last["prod_b"].copies, 4);
        assert_eq!(last["prod_c"].copies, 2);
        assert_eq!(last["prod_c"].at.timestamp_millis(), 1_000);
    }

    #[test]
    fn coverage_status_follows_stock_and_history() {
        let label = Some(LastLabel {
            at: Utc::now(),
            copies: 1,
        });
        assert_eq!(
            coverage_status(5, None, 0),
            Some(LabelCoverageStatus::Never)
        );
        assert_eq!(coverage_status(0, None, 0), None);
        assert_eq!(
            coverage_status(5, label, 3),
            Some(LabelCoverageStatus::Needed)
        );
        assert_eq!(
            coverage_status(5, label, 0),
            Some(LabelCoverageStatus::Labelled)
        );
        assert_eq!(
            coverage_status(0, label, 0),
            Some(LabelCoverageStatus::Labelled)
        );
    }
}
