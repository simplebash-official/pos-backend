//! One-time charges on the embedded SQLite engine: a charge is paid like any other line and shows
//! up in every report that splits revenue by source, so the streams always add up to the sales.

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::helpers::{admin_token, send_authed, setup_sqlite_app};

async fn get(ctx: &crate::helpers::SqliteTestContext, token: &str, uri: &str) -> Value {
    let (status, body) = send_authed(&ctx.router, "GET", uri, None, token).await;
    assert_eq!(status, StatusCode::OK, "{uri} failed: {body}");
    body["data"].clone()
}

#[tokio::test]
async fn charge_revenue_reaches_every_report_and_the_streams_add_up() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);

    // A catalogue item: cost 600, price 1000.
    let (status, category) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Accessories", "icon": "devices", "color": "blue" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{category}");
    let category_key = category["data"]["key"]
        .as_str()
        .expect("the key is a string");
    let (status, subcategory) = send_authed(
        &ctx.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Cases" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{subcategory}");
    let subcategory_key = subcategory["data"]["subcategories"][0]["key"]
        .as_str()
        .expect("the key is a string");
    let (status, product) = send_authed(
        &ctx.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Phone case",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": 5, "unitCostCents": 600, "sellingPriceCents": 1000 },
            "minStockThreshold": 0
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    let product_key = product["data"]["key"]
        .as_str()
        .expect("the key is a string");

    // One line of every source, then 10% off the whole bill: 4400 of lines, 440 off, 3960 paid.
    let (status, sale) = send_authed(
        &ctx.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Default Admin" },
            "items": [
                {
                    "productKey": product_key,
                    "sellingPriceCents": 1000,
                    "quantity": 1,
                    "discountCents": 0,
                    "sourceType": "retail"
                },
                {
                    "name": "Screen fix",
                    "unitPriceCents": 2500,
                    "quantity": 1,
                    "discountCents": 0,
                    "sourceType": "repair"
                },
                {
                    "name": "Poster",
                    "unitPriceCents": 600,
                    "quantity": 1,
                    "discountCents": 0,
                    "sourceType": "print"
                },
                {
                    "name": "Delivery fee",
                    "unitPriceCents": 300,
                    "quantity": 1,
                    "discountCents": 0,
                    "sourceType": "charge"
                }
            ],
            "pricingAdjustments": { "discountType": "percentage", "discountValue": 10 },
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 3960 },
            "shopProfileSnapshot": { "tradingName": "TechFix Repairs" }
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sale}");
    assert_eq!(sale["data"]["invoice"]["subtotalCents"], 4400);
    assert_eq!(sale["data"]["invoice"]["totalCents"], 3960);

    // Dashboard: four streams that add up to the sales before the bill discount.
    let dashboard = get(&ctx, &token, "/api/reports/dashboard?preset=today").await;
    assert_eq!(dashboard["totalRevenueCents"], 3960);
    assert_eq!(dashboard["totalDiscountsCents"], 440);
    assert_eq!(dashboard["retailRevenueCents"], 1000);
    assert_eq!(dashboard["repairRevenueCents"], 2500);
    assert_eq!(dashboard["printRevenueCents"], 600);
    assert_eq!(dashboard["chargeRevenueCents"], 300);

    // Daily sales: the day's charges and the day's total.
    let daily = get(&ctx, &token, "/api/reports/daily-sales").await;
    assert_eq!(daily["totalSalesCents"], 3960);
    assert_eq!(daily["totalChargeRevenueCents"], 300);
    let day = &daily["summaries"][0];
    assert_eq!(day["chargeRevenueCents"], 300);
    let streams = [
        "retailRevenueCents",
        "repairRevenueCents",
        "printRevenueCents",
    ]
    .iter()
    .map(|field| day[field].as_i64().expect("the stream is an integer"))
    .sum::<i64>()
        + day["chargeRevenueCents"]
            .as_i64()
            .expect("the stream is an integer");
    assert_eq!(
        streams,
        day["grossSalesCents"]
            .as_i64()
            .expect("the gross is an integer")
    );

    // Analytics summary: the same four streams, and charges are not retail cost.
    let summary = get(&ctx, &token, "/api/reports/analytics/summary?preset=today").await;
    let current = &summary["current"];
    assert_eq!(current["chargeRevenueCents"], 300);
    assert_eq!(current["retailRevenueCents"], 1000);
    assert_eq!(current["retailCogsCents"], 600);
    let streams: i64 = [
        "retailRevenueCents",
        "repairRevenueCents",
        "printRevenueCents",
        "chargeRevenueCents",
    ]
    .iter()
    .map(|field| current[field].as_i64().expect("the stream is an integer"))
    .sum();
    assert_eq!(
        streams,
        current["totalRevenueCents"]
            .as_i64()
            .expect("the total is an integer")
            + current["discountCents"]
                .as_i64()
                .expect("the discount is an integer")
    );

    // Time series: the bucket and the totals row carry the charge and sum to the line revenue.
    let series = get(
        &ctx,
        &token,
        "/api/reports/analytics/timeseries?preset=today&granularity=day",
    )
    .await;
    let totals = &series["totals"];
    assert_eq!(totals["chargeRevenueCents"], 300);
    assert_eq!(totals["revenueCents"], 4400);

    // Monthly profit: the month's charges.
    let month = get(&ctx, &token, "/api/reports/monthly-profit").await;
    assert_eq!(month["chargeRevenueCents"], 300);
    assert_eq!(month["totalRevenueCents"], 3960);
}
