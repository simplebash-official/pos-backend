// Mongo access for the `employees` collection only. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`
// straight from the driver and leave the "not found" -> `AppError`
// translation to `service`. Visibility is `pub(crate)` so `service` can call
// in, but the `mod repository;` declaration in `employees/mod.rs` is
// private, so none of this is reachable from outside the `employees` module
// tree (see `modules::suppliers::repository` for the same pattern).

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, modules::employees::model::EmployeeDocument};

fn employees(db: &Database) -> Collection<EmployeeDocument> {
    db.collection("employees")
}

pub(crate) async fn find_employee_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<EmployeeDocument>> {
    Ok(employees(db)
        .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
        .await?)
}

/// Looked up by `key` rather than `_id` — the entry point `repairs`/
/// `print_jobs` use to validate an `assignedEmployeeId` and resolve its
/// display name server-side.
pub(crate) async fn find_employee_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<EmployeeDocument>> {
    Ok(employees(db)
        .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
        .await?)
}

/// Fetches every employee matching one of `keys` in a single query — used by
/// `service::get_employees_by_keys` (the batch lookup `reports::commissions`
/// uses to resolve display name/role for a page of commission entries).
pub(crate) async fn find_employees_by_keys(
    db: &Database,
    keys: &[String],
) -> AppResult<Vec<EmployeeDocument>> {
    let mut cursor = employees(db)
        .find(doc! { "key": { "$in": keys }, "deleted_at": { "$exists": false } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Bumps `updated_at`/`version` with no other field change — used when
/// something outside this collection (a login being linked/unlinked) makes
/// the employee's *resolved* API shape (its `login` field) stale without
/// touching the document itself. The sync delta feed pages by `updated_at`,
/// so without this the mirror would never re-deliver the row and an
/// offline client would never learn the employee now has (or lost) a login.
pub(crate) async fn touch_employee_by_key(db: &Database, key: &str) -> AppResult<()> {
    employees(db)
        .update_one(
            doc! { "key": key, "deleted_at": { "$exists": false } },
            doc! {
                "$set": { "updated_at": mongodb::bson::DateTime::now() },
                "$inc": { "version": 1 }
            },
        )
        .await?;
    Ok(())
}

pub(crate) async fn insert_employee(
    db: &Database,
    mut document: EmployeeDocument,
) -> AppResult<EmployeeDocument> {
    let result = employees(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// `set_doc` is built by the caller (`service::create_employee`/
/// `update_employee`), which decides which fields actually change — this
/// just applies it.
pub(crate) async fn update_employee(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<EmployeeDocument>> {
    Ok(employees(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_employee(
    db: &Database,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<EmployeeDocument>> {
    let now = mongodb::bson::DateTime::now();
    let mut set_doc = doc! {
        "deleted_at": now,
        "updated_at": now,
    };
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }
    Ok(employees(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! {
                "$set": set_doc,
                "$inc": { "version": 1 }
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

/// Sorted by name for stable, predictable `GET /employees` output. `filter`
/// is fully assembled by the caller (`service::list_employees`).
pub(crate) async fn list_employees(
    db: &Database,
    filter: Document,
) -> AppResult<Vec<EmployeeDocument>> {
    let mut effective_filter = filter;
    if !effective_filter.contains_key("deleted_at") {
        effective_filter.insert("deleted_at", doc! { "$exists": false });
    }
    let mut cursor = employees(db)
        .find(effective_filter)
        .sort(doc! { "name": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}
