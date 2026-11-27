//! `?search=` on the repair and print job lists, on SQLite.

use serde_json::json;

use crate::helpers::{admin_token, assert_search, create, setup_sqlite_app};

#[tokio::test]
async fn sqlite_search_filters_repairs_and_print_jobs() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);

    let mut repairs = Vec::new();
    let mut print_jobs = Vec::new();
    for (name, phone, device, job_type) in [
        ("Jane Doe", "0771234567", "iPhone (Pro)", "mug"),
        ("Kamal Perera", "0719876543", "Galaxy Tab", "banner"),
    ] {
        let customer_key = create(
            &ctx,
            &token,
            "/api/customers",
            json!({ "name": name, "primaryPhone": phone }),
        )
        .await;
        let customer = json!({ "customerKey": customer_key });
        let repair = json!({
            "customer": customer,
            "deviceModel": device,
            "issueDescription": "Cracked screen",
            "items": [],
        });
        repairs.push(create(&ctx, &token, "/api/repairs", repair).await);
        let print_job = json!({
            "customer": customer,
            "jobType": job_type,
            "quantity": 1,
            "items": [],
        });
        print_jobs.push(create(&ctx, &token, "/api/print-jobs", print_job).await);
    }

    for (target, keys, prefix, item_term) in [
        (("/api/repairs", "repairs"), &repairs, "REP", "pro)"),
        (("/api/print-jobs", "printJobs"), &print_jobs, "PRN", "mug"),
    ] {
        let (jane, kamal) = (&keys[0], &keys[1]);
        assert_search(&ctx, &token, target, "jane", &[jane]).await;
        assert_search(&ctx, &token, target, "PERERA", &[kamal]).await;
        assert_search(&ctx, &token, target, item_term, &[jane]).await;
        assert_search(&ctx, &token, target, "0719876543", &[kamal]).await;
        assert_search(&ctx, &token, target, "nobody", &[]).await;
        // `%` and `_` are text to look for, not wildcards.
        assert_search(&ctx, &token, target, "%25", &[]).await;
        assert_search(&ctx, &token, target, "J_ne", &[]).await;
        let ticket = format!("{prefix}-000002");
        assert_search(&ctx, &token, target, &ticket, &[kamal]).await;
        assert_search(&ctx, &token, target, prefix, &[jane, kamal]).await;
    }
}
