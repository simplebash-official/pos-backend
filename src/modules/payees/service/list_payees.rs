use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::{Payee, PayeeListQuery, PayeeType, normalize_name};
use crate::domain::employees::{EmployeeListQuery, EmployeeStatus};
use crate::domain::suppliers::SupplierListQuery;
use crate::modules::payees::repository;
use crate::modules::{employees, sessions, suppliers};

/// The shared directory, frequent payees first. Suppliers and active staff are read live from
/// their own modules, so a renamed supplier or a new hire shows here at once.
pub async fn list_payees(db: &Db, query: PayeeListQuery) -> AppResult<Vec<Payee>> {
    let wanted = query
        .payee_type
        .as_deref()
        .map(|t| PayeeType::parse(t).ok_or_else(|| AppError::validation("Unknown payee type.")))
        .transpose()?;
    let include_hidden = query.include_hidden.unwrap_or(false);
    let search = query
        .search
        .as_deref()
        .map(normalize_name)
        .filter(|s| !s.is_empty());
    let usage: std::collections::HashMap<String, i64> =
        sessions::service::movement_counts_by_payee(db)
            .await?
            .into_iter()
            .collect();
    let used = |key: &str| usage.get(key).copied().unwrap_or(0);

    let mut out: Vec<Payee> = Vec::new();
    if wanted.is_none_or(|t| t == PayeeType::Supplier) {
        for s in suppliers::service::list_suppliers(db, SupplierListQuery::default())
            .await?
            .suppliers
        {
            out.push(Payee {
                usage_count: used(&s.key),
                key: s.key,
                payee_type: PayeeType::Supplier,
                name: s.name,
                phone: Some(s.primary_phone),
                active: true,
            });
        }
    }
    if wanted.is_none_or(|t| t == PayeeType::Staff) {
        let staff = employees::service::list_employees(
            db,
            EmployeeListQuery {
                status: Some(EmployeeStatus::Active),
                ..Default::default()
            },
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
                active: true,
            });
        }
    }
    for p in repository::list_all(db).await? {
        if wanted.is_none_or(|t| t == p.payee_type) && (include_hidden || p.active) {
            out.push(Payee {
                usage_count: used(&p.key),
                key: p.key,
                payee_type: p.payee_type,
                name: p.name,
                phone: p.phone,
                active: p.active,
            });
        }
    }
    if let Some(q) = &search {
        out.retain(|p| {
            normalize_name(&p.name).contains(q.as_str())
                || p.phone
                    .as_deref()
                    .is_some_and(|ph| normalize_name(ph).contains(q.as_str()))
        });
    }
    out.sort_by(|a, b| b.usage_count.cmp(&a.usage_count).then(a.name.cmp(&b.name)));
    Ok(out)
}
