use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        middleware::auth::CurrentUser,
        utils::calculate_pagination,
    },
    domain::{
        imports::{
            ImportBatch, ImportBatchListQuery, ImportBatchListResponse, ProcessImportRequest,
        },
        inventory::PaginationMeta,
    },
    modules::imports::{
        model::{ImportBatchDocument, ImportRowErrorDocument},
        repository,
        service::handlers::inventory::process_inventory_import,
    },
};

pub(crate) async fn process_import(
    db: &Db,
    user: &CurrentUser,
    body: ProcessImportRequest,
) -> AppResult<ImportBatch> {
    crate::core::logging::domain::tracked("imports.processed", async move {
        let target = body.target.trim().to_lowercase();
        let file_name = body.file_name.trim().to_string();
        let file_type = body.file_type.trim().to_lowercase();

        if target.is_empty() {
            return Err(AppError::validation("Target module is required"));
        }
        if file_name.is_empty() {
            return Err(AppError::validation("File name is required"));
        }
        if body.rows.is_empty() {
            return Err(AppError::validation(
                "Cannot import empty file: no rows provided",
            ));
        }

        // Permission check based on target
        match target.as_str() {
            "inventory" => {
                user.require_permission(crate::core::constants::permissions::INVENTORY_WRITE)?;
            }
            _ => {
                return Err(AppError::custom(
                    axum::http::StatusCode::BAD_REQUEST,
                    codes::IMPORT_TARGET_UNSUPPORTED,
                    format!("Import target '{target}' is not supported"),
                ));
            }
        }

        let batch_key = generate_id(prefixes::IMPORT_BATCH);
        let total_rows = body.rows.len() as u64;
        let now = mongodb::bson::DateTime::now();

        // 1. Initial batch insertion with status "processing"
        let initial_doc = ImportBatchDocument {
            id: None,
            key: batch_key.clone(),
            target: target.clone(),
            file_name: file_name.clone(),
            file_type: file_type.clone(),
            file_size_bytes: body.file_size_bytes,
            total_rows,
            successful_rows: 0,
            failed_rows: 0,
            status: "processing".to_string(),
            errors: Vec::new(),
            created_by_user_key: Some(user.user_id.clone()),
            created_by_user_name: None,
            created_at: now,
            updated_at: now,
        };

        let inserted_doc = repository::batch::insert_batch(db, initial_doc).await?;

        let options = body.options.unwrap_or_default();

        // 2. Dispatch to target handler
        let (successful_count, failed_count, errors) = match target.as_str() {
            "inventory" => {
                let res =
                    process_inventory_import(db, &body.rows, &options, &file_name, &batch_key)
                        .await?;
                (res.successful_count, res.failed_count, res.errors)
            }
            _ => unreachable!(),
        };

        // 3. Determine final status
        let status = if failed_count == 0 {
            "completed".to_string()
        } else if successful_count == 0 {
            "failed".to_string()
        } else {
            "partially_completed".to_string()
        };

        let doc_errors = errors
            .into_iter()
            .map(ImportRowErrorDocument::from_row_error)
            .collect();

        let updated_doc = repository::batch::update_batch_result(
            db,
            &batch_key,
            successful_count,
            failed_count,
            &status,
            doc_errors,
        )
        .await?
        .unwrap_or(inserted_doc);

        Ok(updated_doc.into_import_batch())
    })
    .await
}

pub(crate) async fn get_import_batch(db: &Db, key: &str) -> AppResult<ImportBatch> {
    let doc = repository::batch::find_batch_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Import batch not found", codes::IMPORT_BATCH_NOT_FOUND)
        })?;

    Ok(doc.into_import_batch())
}

pub(crate) async fn list_import_batches(
    db: &Db,
    query: ImportBatchListQuery,
) -> AppResult<ImportBatchListResponse> {
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 100);
    let (docs, total) =
        repository::batch::list_batches(db, query.target.as_deref(), skip, limit).await?;

    let total_pages = if total == 0 {
        1
    } else {
        (total as f64 / limit as f64).ceil() as u64
    };

    let pagination = PaginationMeta {
        page,
        limit,
        total,
        total_pages,
    };

    let items = docs
        .into_iter()
        .map(ImportBatchDocument::into_import_batch)
        .collect();

    Ok(ImportBatchListResponse { items, pagination })
}
