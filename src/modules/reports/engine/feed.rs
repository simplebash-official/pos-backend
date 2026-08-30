// Multi-dataset aggregation service for the Analytics & Chart Engine feed.
// Uses `tokio::try_join!` for high-concurrency database execution, reducing
// frontend roundtrips from 15 to 1.

use mongodb::Database;
use tokio::try_join;

use crate::{
    core::error::AppResult,
    domain::reports::{
        AnalyticsRangeQuery, CustomersFeedData, DailySalesQuery, EmployeeCommissionsQuery,
        EngineFeedQuery, EngineFeedResponse, EngineMetadata, MonthlyProfitQuery, OverviewFeedData,
        ProfitFeedData, ReceivablesAgingQuery, SalesByCategoryQuery, SalesFeedData, StaffFeedData,
        TopCustomersQuery, TopProductsQuery,
    },
    modules::reports::service::{self, dates::parse_date_range_tz},
};

pub async fn build_engine_feed(
    db: &Database,
    query: &EngineFeedQuery,
    meta: EngineMetadata,
) -> AppResult<EngineFeedResponse> {
    let section = query
        .section
        .as_deref()
        .unwrap_or("overview")
        .to_lowercase();
    let (period_start, period_end) = parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;

    let range_query = AnalyticsRangeQuery {
        preset: query.preset.clone(),
        from: query.from.clone(),
        to: query.to.clone(),
        granularity: query.granularity.clone(),
        compare_previous: query.compare_previous,
    };

    let mut response = EngineFeedResponse {
        section: section.clone(),
        period_start,
        period_end,
        meta,
        overview: None,
        sales: None,
        profit: None,
        customers: None,
        staff: None,
    };

    match section.as_str() {
        "overview" => {
            let overview = build_overview_feed(db, &range_query).await?;
            response.overview = Some(overview);
        }
        "sales" => {
            let sales = build_sales_feed(db, query, &range_query).await?;
            response.sales = Some(sales);
        }
        "profit" => {
            let profit = build_profit_feed(db, query, &range_query).await?;
            response.profit = Some(profit);
        }
        "customers" => {
            let customers = build_customers_feed(db, query).await?;
            response.customers = Some(customers);
        }
        "staff" => {
            let staff = build_staff_feed(db, query, &range_query).await?;
            response.staff = Some(staff);
        }
        "all" => {
            let (overview, sales, profit, customers, staff) = try_join!(
                build_overview_feed(db, &range_query),
                build_sales_feed(db, query, &range_query),
                build_profit_feed(db, query, &range_query),
                build_customers_feed(db, query),
                build_staff_feed(db, query, &range_query),
            )?;
            response.overview = Some(overview);
            response.sales = Some(sales);
            response.profit = Some(profit);
            response.customers = Some(customers);
            response.staff = Some(staff);
        }
        _ => {
            let overview = build_overview_feed(db, &range_query).await?;
            response.overview = Some(overview);
        }
    }

    Ok(response)
}

async fn build_overview_feed(
    db: &Database,
    range_query: &AnalyticsRangeQuery,
) -> AppResult<OverviewFeedData> {
    let (summary, timeseries, payment_methods, sales_patterns) = try_join!(
        service::get_analytics_summary(db, range_query.clone()),
        service::get_analytics_timeseries(db, range_query.clone()),
        service::get_analytics_payment_methods(db, range_query.clone()),
        service::get_analytics_sales_patterns(db, range_query.clone()),
    )?;

    Ok(OverviewFeedData {
        summary,
        timeseries,
        payment_methods,
        sales_patterns,
    })
}

async fn build_sales_feed(
    db: &Database,
    query: &EngineFeedQuery,
    range_query: &AnalyticsRangeQuery,
) -> AppResult<SalesFeedData> {
    let daily_query = DailySalesQuery {
        date: None,
        from: query.from.clone(),
        to: query.to.clone(),
    };

    let cat_query = SalesByCategoryQuery {
        preset: query.preset.clone(),
        from: query.from.clone(),
        to: query.to.clone(),
        group_by: query.group_by.clone(),
    };

    let top_prod_query = TopProductsQuery {
        preset: query.preset.clone(),
        from: query.from.clone(),
        to: query.to.clone(),
        limit: query.limit.or(Some(100)),
        sort_by: query.sort_by.clone(),
    };

    let (daily_sales, sales_by_category, discounts, refunds, top_products) = try_join!(
        service::get_daily_sales_report(db, daily_query),
        service::get_analytics_sales_by_category(db, cat_query),
        service::get_analytics_discounts(db, range_query.clone()),
        service::get_analytics_refunds(db, range_query.clone()),
        service::get_top_products(db, top_prod_query),
    )?;

    Ok(SalesFeedData {
        daily_sales,
        sales_by_category,
        discounts,
        refunds,
        top_products,
    })
}

async fn build_profit_feed(
    db: &Database,
    _query: &EngineFeedQuery,
    range_query: &AnalyticsRangeQuery,
) -> AppResult<ProfitFeedData> {
    let profit_query = MonthlyProfitQuery {
        month: None,
        year: None,
    };

    let (monthly_profit, timeseries) = try_join!(
        service::get_monthly_profit_report(db, profit_query),
        service::get_analytics_timeseries(db, range_query.clone()),
    )?;

    Ok(ProfitFeedData {
        monthly_profit,
        timeseries,
    })
}

async fn build_customers_feed(
    db: &Database,
    query: &EngineFeedQuery,
) -> AppResult<CustomersFeedData> {
    let top_cust_query = TopCustomersQuery {
        preset: query.preset.clone(),
        from: query.from.clone(),
        to: query.to.clone(),
        limit: query.limit.or(Some(100)),
        sort_by: query.sort_by.clone(),
    };

    let aging_query = ReceivablesAgingQuery { as_of: None };

    let (top_customers, receivables_aging) = try_join!(
        service::get_analytics_top_customers(db, top_cust_query),
        service::get_analytics_receivables_aging(db, aging_query),
    )?;

    Ok(CustomersFeedData {
        top_customers,
        receivables_aging,
    })
}

async fn build_staff_feed(
    db: &Database,
    query: &EngineFeedQuery,
    range_query: &AnalyticsRangeQuery,
) -> AppResult<StaffFeedData> {
    let comm_query = EmployeeCommissionsQuery {
        preset: query.preset.clone(),
        from: query.from.clone(),
        to: query.to.clone(),
        employee_key: None,
    };

    let (employee_commissions, cashier_performance) = try_join!(
        service::get_employee_commissions_report(db, comm_query),
        service::get_analytics_cashier_performance(db, range_query.clone()),
    )?;

    Ok(StaffFeedData {
        employee_commissions,
        cashier_performance,
    })
}
