// Business rules for employee HR/commission profile CRUD/listing: required-
// field validation, the login-summary enrichment every read performs (a
// cross-module call into `modules::users`), and the "still has a login"
// guard that blocks deleting an employee while a login account still
// references it. Delegates all Mongo access to `super::repository`.

use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::build_bson_regex,
    },
    domain::employees::{
        CreateEmployeeRequest, Employee, EmployeeListQuery, EmployeeStatus, EmployeesResponse,
        UpdateEmployeeRequest,
    },
    modules::{
        employees::{model::EmployeeDocument, repository},
        users,
    },
};

/// Name/phone/commission-split invariants shared by create and update.
fn validate_required_fields(name: &str, phone: &str, default_split_value: f64) -> AppResult<()> {
    if name.trim().chars().count() < 2 {
        return Err(AppError::validation(
            "Employee name must be at least 2 characters",
        ));
    }
    if phone.trim().is_empty() {
        return Err(AppError::validation("Phone is required"));
    }
    if !phone
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(
            "Phone may only contain digits, spaces, dashes, and a plus sign",
        ));
    }
    if default_split_value < 0.0 {
        return Err(AppError::validation(
            "Default commission split value cannot be negative",
        ));
    }
    Ok(())
}

/// Builds the Mongo filter from query params (free-text search across
/// name/phone/nicOrId, plus exact role/status filters) — same construction
/// style as `suppliers::service::list_suppliers`. Not paginated: a shop's
/// staff roster is small enough to return in full.
pub async fn list_employees(
    db: &Database,
    query: EmployeeListQuery,
) -> AppResult<EmployeesResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = build_bson_regex(&search);
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "phone": { "$regex": pattern.clone() } },
                { "nic_or_id": { "$regex": pattern } },
            ]
        });
    }
    if let Some(role) = query.role {
        and_clauses.push(doc! { "role": role.as_str() });
    }
    if let Some(status) = query.status {
        and_clauses.push(doc! { "status": status.as_str() });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let documents = repository::list_employees(db, filter).await?;
    let keys: Vec<String> = documents.iter().map(|d| d.key.clone()).collect();
    let mut logins = users::service::find_user_summaries_by_employee_keys(db, &keys).await?;

    let employees = documents
        .into_iter()
        .map(|document| {
            let login = logins.remove(&document.key);
            document.into_employee(login)
        })
        .collect();

    Ok(EmployeesResponse { employees })
}

/// Fetch by id, 404ing with the module-specific `EMPLOYEE_NOT_FOUND` code
/// rather than the generic `NOT_FOUND`.
pub(crate) async fn get_employee(db: &Database, id: ObjectId) -> AppResult<Employee> {
    let document = repository::find_employee_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;
    let login = users::service::find_user_summary_by_employee_key(db, &document.key).await?;

    Ok(document.into_employee(login))
}

/// Same as `get_employee`, looked up by `key` instead of `ObjectId` — the
/// cross-module entry point `repairs`/`print_jobs` call to resolve an
/// `assignedEmployeeId` and derive its display name server-side.
pub(crate) async fn get_employee_by_key(db: &Database, key: &str) -> AppResult<Employee> {
    let document = repository::find_employee_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;
    let login = users::service::find_user_summary_by_employee_key(db, &document.key).await?;

    Ok(document.into_employee(login))
}

/// Cross-module entry point `users::service` calls after linking/unlinking
/// an `employeeKey` on a login (create, update, or delete) — see
/// `repository::touch_employee_by_key` for why this is necessary at all.
/// Silently does nothing if `key` no longer resolves to a live employee
/// (e.g. the employee was deleted between the two operations); that's not
/// this caller's problem to raise.
pub(crate) async fn touch_by_key(db: &Database, key: &str) -> AppResult<()> {
    repository::touch_employee_by_key(db, key).await
}

/// Cross-module batch lookup — `reports::repository::commissions`' aggregation
/// calls this to resolve display name/role for a page of commission entries
/// keyed by `assignedEmployeeId`, in one query instead of one lookup per row
/// (the "reach another module through its service, never $lookup across a
/// module boundary" pattern `purchases`/`supplier_products` already use).
pub(crate) async fn get_employees_by_keys(
    db: &Database,
    keys: &[String],
) -> AppResult<Vec<Employee>> {
    let documents = repository::find_employees_by_keys(db, keys).await?;
    let doc_keys: Vec<String> = documents.iter().map(|d| d.key.clone()).collect();
    let mut logins = users::service::find_user_summaries_by_employee_keys(db, &doc_keys).await?;

    Ok(documents
        .into_iter()
        .map(|document| {
            let login = logins.remove(&document.key);
            document.into_employee(login)
        })
        .collect())
}

pub async fn create_employee(db: &Database, body: CreateEmployeeRequest) -> AppResult<Employee> {
    validate_required_fields(&body.name, &body.phone, body.default_split_value)?;

    let now = BsonDateTime::now();
    let document = EmployeeDocument {
        id: None,
        key: generate_id(prefixes::EMPLOYEE),
        name: body.name,
        phone: body.phone,
        nic_or_id: body.nic_or_id,
        role: body.role,
        default_split_type: body.default_split_type,
        default_split_value: body.default_split_value,
        status: body.status.unwrap_or(EmployeeStatus::Active),
        notes: body.notes,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        updated_by_device: None,
    };

    let inserted = repository::insert_employee(db, document).await?;
    Ok(inserted.into_employee(None))
}

/// Partial update for `PATCH /employees/{id}` — every field in `body` is
/// optional; required fields (name/phone/defaultSplitValue) fall back to the
/// existing document's value before re-validation, while genuinely-optional
/// fields (nicOrId/notes) are only touched when explicitly provided (same
/// convention as `suppliers::service::update_supplier`).
pub(crate) async fn update_employee(
    db: &Database,
    id: ObjectId,
    body: UpdateEmployeeRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Employee> {
    let existing = repository::find_employee_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;

    if let Some(expected) = expected_version
        && existing.version != expected
    {
        let mut conflicting = Vec::new();
        if let Some(ref name) = body.name
            && name != &existing.name
        {
            conflicting.push("name");
        }
        if let Some(ref phone) = body.phone
            && phone != &existing.phone
        {
            conflicting.push("phone");
        }
        if let Some(ref split_value) = body.default_split_value
            && *split_value != existing.default_split_value
        {
            conflicting.push("defaultSplitValue");
        }

        let login = users::service::find_user_summary_by_employee_key(db, &existing.key).await?;
        return Err(AppError::conflict_with_details(
            codes::VERSION_CONFLICT,
            "This employee was changed on another device.",
            serde_json::json!({
                "expectedVersion": expected,
                "serverVersion": existing.version,
                "updatedByDevice": existing.updated_by_device,
                "server": existing.into_employee(login),
                "conflictingFields": conflicting,
            }),
        ));
    }

    let name = body.name.unwrap_or(existing.name);
    let phone = body.phone.unwrap_or(existing.phone);
    let default_split_value = body
        .default_split_value
        .unwrap_or(existing.default_split_value);

    validate_required_fields(&name, &phone, default_split_value)?;

    let mut set_doc = doc! {
        "name": &name,
        "phone": &phone,
        "default_split_value": default_split_value,
        "updated_at": BsonDateTime::now(),
    };
    if let Some(nic_or_id) = body.nic_or_id {
        set_doc.insert("nic_or_id", nic_or_id);
    }
    if let Some(role) = body.role {
        set_doc.insert("role", role.as_str());
    }
    if let Some(split_type) = body.default_split_type {
        set_doc.insert("default_split_type", split_type.as_str());
    }
    if let Some(status) = body.status {
        set_doc.insert("status", status.as_str());
    }
    if let Some(notes) = body.notes {
        set_doc.insert("notes", notes);
    }
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }

    let updated = repository::update_employee(db, id, set_doc)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;
    let login = users::service::find_user_summary_by_employee_key(db, &updated.key).await?;

    Ok(updated.into_employee(login))
}

/// Refuses to delete (409 `EMPLOYEE_HAS_LOGIN`) while this employee still
/// has a linked login account — the login must be removed first, mirroring
/// `suppliers::service::delete_supplier`'s `SUPPLIER_HAS_PURCHASES` guard.
pub(crate) async fn delete_employee(
    db: &Database,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Employee> {
    let existing = repository::find_employee_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;

    if users::service::find_user_summary_by_employee_key(db, &existing.key)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            codes::EMPLOYEE_HAS_LOGIN,
            "This employee still has a login account. Remove their login first.",
        ));
    }

    let deleted = repository::delete_employee(db, id, device_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Employee not found", codes::EMPLOYEE_NOT_FOUND)
        })?;

    Ok(deleted.into_employee(None))
}

/// Batch delete for `DELETE /employees/batch`. Ids that aren't valid
/// `ObjectId`s, that don't resolve to an existing employee, or that are
/// blocked by the `EMPLOYEE_HAS_LOGIN` guard are silently skipped rather
/// than failing the whole request — same "valid ones still succeed"
/// semantics as `suppliers::service::delete_suppliers`.
pub(crate) async fn delete_employees(db: &Database, ids: Vec<String>) -> AppResult<u64> {
    let mut deleted_count = 0u64;
    for id in ids {
        if let Ok(object_id) = ObjectId::parse_str(&id)
            && delete_employee(db, object_id, None).await.is_ok()
        {
            deleted_count += 1;
        }
    }
    Ok(deleted_count)
}

/// Converts a page of raw `employees` documents — as read by the sync
/// module's cursor scan — into the `Employee` shape the REST reads return.
/// Unlike every other module's `hydrate_sync_documents`, this one is `async`
/// and takes `db`: the `login` field needs the same cross-module lookup a
/// REST read performs, which a pure deserialize-and-convert can't do. See
/// `suppliers::service::hydrate_sync_documents` for why the delta and
/// snapshot feeds must otherwise produce identical rows.
pub(crate) async fn hydrate_sync_documents(
    db: &Database,
    documents: Vec<Document>,
) -> AppResult<Vec<Employee>> {
    let docs: Vec<EmployeeDocument> = documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<EmployeeDocument>(
                document,
            )?)
        })
        .collect::<AppResult<Vec<_>>>()?;

    let keys: Vec<String> = docs.iter().map(|d| d.key.clone()).collect();
    let mut logins = users::service::find_user_summaries_by_employee_keys(db, &keys).await?;

    Ok(docs
        .into_iter()
        .map(|document| {
            let login = logins.remove(&document.key);
            document.into_employee(login)
        })
        .collect())
}
