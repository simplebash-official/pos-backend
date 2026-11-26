//! The shop's ready-made print services: a name (unique per shop, ignoring case and spaces;
//! the key is derived from it, see `print_service_key`), the usual price of one unit and the
//! usual warranty. Never deleted: a job that used one keeps its own copy of the line, and a
//! service no longer offered is hidden (`active: false`).

use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::normalize_name;
use crate::domain::print_jobs::{PrintService, SavePrintServiceRequest, print_service_key};
use crate::modules::print_jobs::model::PrintServiceDocument;
use crate::modules::print_jobs::repository::print_services_repository as print_services;
use mongodb::bson::DateTime as BsonDateTime;

const NAME_MAX: usize = 80;

/// The shop's services by name; hidden ones only when asked.
pub(crate) async fn list_services(db: &Db, include_hidden: bool) -> AppResult<Vec<PrintService>> {
    Ok(print_services::list(db)
        .await?
        .into_iter()
        .filter(|service| include_hidden || service.active)
        .map(PrintServiceDocument::into_print_service)
        .collect())
}

/// A checked request: name trimmed, price not negative, warranty valid.
fn checked(body: SavePrintServiceRequest) -> AppResult<SavePrintServiceRequest> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::validation("Give the service a name."));
    }
    if name.chars().count() > NAME_MAX {
        return Err(AppError::validation(format!(
            "A service name can be at most {NAME_MAX} characters."
        )));
    }
    if body.unit_price_cents < 0 {
        return Err(AppError::validation("The price cannot be negative."));
    }
    Ok(SavePrintServiceRequest {
        name,
        unit_price_cents: body.unit_price_cents,
        warranty: body.warranty.validated()?,
        active: body.active,
    })
}

/// Refuses a name another service of the shop already has (spaces and case ignored), hidden
/// ones included. A new name is also refused when its derived key belongs to a service that was
/// since renamed, since the key is what makes two tills' copies one row.
async fn ensure_name_free(db: &Db, name: &str, own_key: Option<&str>) -> AppResult<()> {
    let wanted = normalize_name(name);
    let new_key = print_service_key(name);
    let taken = print_services::list(db).await?.into_iter().find(|service| {
        Some(service.key.as_str()) != own_key
            && (normalize_name(&service.name) == wanted
                || (own_key.is_none() && service.key == new_key))
    });
    match taken {
        Some(service) => Err(AppError::conflict(
            codes::PRINT_SERVICE_NAME_TAKEN,
            if service.active {
                format!("There is already a service called {}.", service.name)
            } else {
                format!(
                    "There is already a hidden service called {}. Show it again instead.",
                    service.name
                )
            },
        )),
        None => Ok(()),
    }
}

/// Adds a service.
pub(crate) async fn create_service(
    db: &Db,
    body: SavePrintServiceRequest,
    device_id: Option<String>,
) -> AppResult<PrintService> {
    crate::core::logging::domain::tracked("print_jobs.service_created", async move {
        let body = checked(body)?;
        ensure_name_free(db, &body.name, None).await?;
        let now = BsonDateTime::now();
        let document = PrintServiceDocument {
            id: None,
            key: print_service_key(&body.name),
            name: body.name,
            unit_price_cents: body.unit_price_cents,
            warranty: body.warranty,
            active: body.active,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: device_id,
        };
        Ok(print_services::insert(db, document)
            .await?
            .into_print_service())
    })
    .await
}

/// Replaces a service's name, price, warranty and whether it is offered.
pub(crate) async fn update_service(
    db: &Db,
    key: &str,
    body: SavePrintServiceRequest,
    device_id: Option<String>,
) -> AppResult<PrintService> {
    crate::core::logging::domain::tracked("print_jobs.service_updated", async move {
        let body = checked(body)?;
        let mut document = print_services::find(db, key).await?.ok_or_else(|| {
            AppError::not_found_with_code("Print service not found", codes::PRINT_SERVICE_NOT_FOUND)
        })?;
        ensure_name_free(db, &body.name, Some(key)).await?;
        let expected = document.version;
        document.name = body.name;
        document.unit_price_cents = body.unit_price_cents;
        document.warranty = body.warranty;
        document.active = body.active;
        document.version = expected + 1;
        document.updated_at = BsonDateTime::now();
        document.updated_by_device = device_id;
        if !print_services::replace(db, &document, expected).await? {
            return Err(AppError::conflict(
                codes::VERSION_CONFLICT,
                "Someone else changed this service just now. Open it again and retry.",
            ));
        }
        Ok(document.into_print_service())
    })
    .await
}

/// Converts a page of raw `print_service_templates` documents for the sync feed.
pub(crate) fn hydrate_sync_services(
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<PrintService>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(
                bson::deserialize_from_document::<PrintServiceDocument>(document)?
                    .into_print_service(),
            )
        })
        .collect()
}
