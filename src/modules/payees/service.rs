// Business rules for the payee directory: one list over suppliers, staff and stored
// payees, name checks, and resolving a payee key from a cash movement.

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        logging::domain::tracked,
    },
    domain::{
        cash_categories::{CreatePayeeRequest, Payee, PayeeListQuery, PayeeType, normalize_name, payee_key},
        employees::{EmployeeListQuery, EmployeeStatus},
        suppliers::SupplierListQuery,
    },
    modules::{
        employees, payees::{model::PayeeDocument, repository}, sessions, suppliers,
    },
};

const NAME_MIN: usize = 2;
const NAME_MAX: usize = 60;

fn clean_name(raw: &str) -> AppResult<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let len = name.chars().count();
    if !(NAME_MIN..=NAME_MAX).contains(&len) {
        return Err(AppError::validation(format!(
            "A name needs {NAME_MIN} to {NAME_MAX} letters."
        )));
    }
    Ok(name)
}

/// Who a payee key refers to, resolved from the module that owns it.
pub(crate) struct ResolvedPayee {
    pub payee_type: PayeeType,
    pub name: String,
}

/// Looks a payee key up: `sup_…` in suppliers, `emp_…` in staff, `pye_…` in stored payees.
pub(crate) async fn resolve_payee(db: &Db, key: &str) -> AppResult<ResolvedPayee> {
    let not_found = || AppError::not_found_with_code("Payee not found.", codes::PAYEE_NOT_FOUND);
    if key.starts_with(&format!("{}_", prefixes::SUPPLIER)) {
        let supplier = suppliers::service::get_supplier_by_key(db, key).await?;
        return Ok(ResolvedPayee { payee_type: PayeeType::Supplier, name: supplier.name });
    }
    if key.starts_with(&format!("{}_", prefixes::EMPLOYEE)) {
        let employee = employees::service::get_employee_by_key(db, key).await?;
        return Ok(ResolvedPayee { payee_type: PayeeType::Staff, name: employee.name });
    }
    if key.starts_with(&format!("{}_", prefixes::PAYEE)) {
        let payee = repository::find_by_key(db, key).await?.ok_or_else(not_found)?;
        return Ok(ResolvedPayee { payee_type: payee.payee_type, name: payee.name });
    }
    Err(not_found())
}

/// The shared directory, frequent payees first. Suppliers and active staff are read live from
/// their own modules, so a renamed supplier or a new hire shows here at once.
pub async fn list_payees(db: &Db, query: PayeeListQuery) -> AppResult<Vec<Payee>> {
    let wanted = query
        .payee_type
        .as_deref()
        .map(|t| PayeeType::parse(t).ok_or_else(|| AppError::validation("Unknown payee type.")))
        .transpose()?;
    let search = query.search.as_deref().map(normalize_name).filter(|s| !s.is_empty());
    let usage: std::collections::HashMap<String, i64> =
        sessions::service::movement_counts_by_payee(db).await?.into_iter().collect();
    let used = |key: &str| usage.get(key).copied().unwrap_or(0);

    let mut out: Vec<Payee> = Vec::new();
    if wanted.is_none_or(|t| t == PayeeType::Supplier) {
        for s in suppliers::service::list_suppliers(db, SupplierListQuery::default()).await?.suppliers {
            out.push(Payee {
                usage_count: used(&s.key),
                key: s.key,
                payee_type: PayeeType::Supplier,
                name: s.name,
                phone: Some(s.primary_phone),
            });
        }
    }
    if wanted.is_none_or(|t| t == PayeeType::Staff) {
        let staff = employees::service::list_employees(
            db,
            EmployeeListQuery { status: Some(EmployeeStatus::Active), ..Default::default() },
        )
        .await?
        .employees;
        for e in staff {
            out.push(Payee {
                usage_count: used(&e.key),
                key: e.key,
                payee_type: PayeeType::Staff,
                name: e.name,
                phone: Some(e.phone),
            });
        }
    }
    for p in repository::list_all(db).await? {
        if wanted.is_none_or(|t| t == p.payee_type) {
            out.push(Payee {
                usage_count: used(&p.key),
                key: p.key,
                payee_type: p.payee_type,
                name: p.name,
                phone: p.phone,
            });
        }
    }
    if let Some(q) = &search {
        out.retain(|p| {
            normalize_name(&p.name).contains(q.as_str())
                || p.phone.as_deref().is_some_and(|ph| normalize_name(ph).contains(q.as_str()))
        });
    }
    out.sort_by(|a, b| b.usage_count.cmp(&a.usage_count).then(a.name.cmp(&b.name)));
    Ok(out)
}

fn new_document(payee_type: PayeeType, name: String, phone: Option<String>) -> PayeeDocument {
    let now = BsonDateTime::now();
    PayeeDocument {
        id: None,
        key: payee_key(payee_type, &name),
        payee_type,
        normalized_name: normalize_name(&name),
        name,
        phone,
        version: 1,
        created_at: now,
        updated_at: now,
        updated_by_device: None,
    }
}

/// Adds a landlord, utility company or other payee on purpose (a manager's job).
pub async fn create_payee(db: &Db, body: CreatePayeeRequest) -> AppResult<Payee> {
    tracked("payees.created", async move {
        if !matches!(body.payee_type, PayeeType::Vendor | PayeeType::Other) {
            return Err(AppError::validation(
                "Suppliers and staff are added on their own pages.",
            ));
        }
        let name = clean_name(&body.name)?;
        let phone = body
            .phone
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
        let document = new_document(body.payee_type, name, phone);
        if repository::find_by_key(db, &document.key).await?.is_some() {
            return Err(AppError::conflict(
                codes::PAYEE_NAME_TAKEN,
                "Someone with this name is already in the list.",
            ));
        }
        let inserted = repository::insert(db, document).await?;
        Ok(Payee {
            key: inserted.key,
            payee_type: inserted.payee_type,
            name: inserted.name,
            phone: inserted.phone,
            usage_count: 0,
        })
    })
    .await
}

/// A name typed once while paying a small expense: found if already saved, else saved for next time.
pub(crate) async fn ensure_other_payee(db: &Db, raw_name: &str) -> AppResult<PayeeDocument> {
    let name = clean_name(raw_name)?;
    let document = new_document(PayeeType::Other, name, None);
    if let Some(existing) = repository::find_by_key(db, &document.key).await? {
        return Ok(existing);
    }
    repository::insert(db, document).await
}
