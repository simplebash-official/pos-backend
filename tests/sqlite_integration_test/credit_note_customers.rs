//! The one-time copy of a customer onto credit notes saved without one, at SQLite startup.

use serde_json::{Value, json};
use simplebash_pos_backend::clients;
use simplebash_pos_backend::core::logging::SqlLogging;
use sqlx::SqlitePool;

use crate::helpers::{SqliteTestContext, admin_token, create, send_authed, setup_sqlite_app};

/// Sells one charge line, returns it and gives back the credit note's key.
async fn returned_sale(ctx: &SqliteTestContext, token: &str, sale: Value) -> String {
    let (status, sold) =
        send_authed(&ctx.router, "POST", "/api/billing/sales", Some(sale), token).await;
    assert!(status.is_success(), "Body: {sold}");
    let returned = json!({
        "invoiceKey": sold["data"]["invoice"]["key"],
        "returnedItems": [{ "name": "Delivery", "quantity": 1,
                            "condition": "resalable", "reason": "defective" }],
        "paymentMethod": "cash",
    });
    let key = create(ctx, token, "/api/billing/credit-notes", returned).await;
    key.as_str().expect("the key is a string").to_string()
}

async fn queued_credit_notes(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox WHERE resource = 'creditNotes'")
        .fetch_one(pool)
        .await
        .expect("the outbox counts")
}

#[tokio::test]
async fn sqlite_startup_gives_old_credit_notes_their_customer() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);
    let customer = json!({ "name": "Jane Doe", "primaryPhone": "0771234567" });
    let customer_key = create(&ctx, &token, "/api/customers", customer).await;
    let item = json!([{ "name": "Delivery", "sourceType": "charge", "unitPriceCents": 500,
                        "quantity": 1, "discountCents": 0 }]);
    let shop = json!({ "tradingName": "TechFix Repairs" });
    let named = returned_sale(
        &ctx,
        &token,
        json!({
            "staff": { "cashierName": "Default Admin" },
            "customer": { "customerKey": customer_key },
            "items": item,
            "payment": { "paymentMethod": "cash", "isCredit": true },
            "shopProfileSnapshot": shop,
        }),
    )
    .await;
    let walk_in = returned_sale(
        &ctx,
        &token,
        json!({
            "staff": { "cashierName": "Default Admin" },
            "items": item,
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 500 },
            "shopProfileSnapshot": shop,
        }),
    )
    .await;

    // Make the database look like one written before credit notes copied the customer.
    let url = format!("sqlite://{}", ctx.db_path.display());
    let pool = clients::sqlite::connect(&url, SqlLogging::Off)
        .await
        .expect("the database opens");
    sqlx::query("UPDATE credit_notes SET customer_key = NULL, customer_name_snapshot = NULL")
        .execute(&pool)
        .await
        .expect("the customers are forgotten");
    sqlx::query("UPDATE sync_state SET capture_enabled = 1 WHERE id = 1")
        .execute(&pool)
        .await
        .expect("sync capture turns on");
    assert_eq!(queued_credit_notes(&pool).await, 0);

    // A restart.
    clients::sqlite::init_db(&pool)
        .await
        .expect("the database starts again");

    for (key, name, owner) in [
        (&named, json!("Jane Doe"), customer_key),
        (&walk_in, Value::Null, Value::Null),
    ] {
        let uri = format!("/api/billing/credit-notes/{key}");
        let (status, body) = send_authed(&ctx.router, "GET", &uri, None, &token).await;
        assert!(status.is_success(), "Body: {body}");
        assert_eq!(body["data"]["customerKey"], owner, "Body: {body}");
        assert_eq!(body["data"]["customerNameSnapshot"], name, "Body: {body}");
    }
    // The cloud copies the customer itself, so nothing is uploaded; later edits still are.
    assert_eq!(queued_credit_notes(&pool).await, 0);
    sqlx::query("UPDATE credit_notes SET notes = 'checked' WHERE key = ?")
        .bind(&named)
        .execute(&pool)
        .await
        .expect("the note is edited");
    assert_eq!(queued_credit_notes(&pool).await, 1);
}
