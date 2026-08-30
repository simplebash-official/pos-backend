// Compound index setup for MongoDB collections to accelerate analytical queries.

use mongodb::{Database, IndexModel, bson::doc, options::IndexOptions};
use tracing::{info, warn};

pub async fn ensure_analytics_indexes(db: &Database) {
    // 1. Invoices indexes
    let invoice_indexes = vec![
        IndexModel::builder()
            .keys(doc! { "status": 1, "created_at": -1 })
            .options(
                IndexOptions::builder()
                    .name(Some("idx_invoices_status_created".to_string()))
                    .build(),
            )
            .build(),
        IndexModel::builder()
            .keys(doc! { "status": 1, "customer_key": 1, "created_at": -1 })
            .options(
                IndexOptions::builder()
                    .name(Some("idx_invoices_status_customer".to_string()))
                    .build(),
            )
            .build(),
    ];

    if let Err(e) = db
        .collection::<mongodb::bson::Document>("invoices")
        .create_indexes(invoice_indexes)
        .await
    {
        warn!("Failed to create analytics indexes on invoices: {e}");
    }

    // 2. Credit notes indexes
    let credit_note_indexes = vec![
        IndexModel::builder()
            .keys(doc! { "status": 1, "created_at": -1 })
            .options(
                IndexOptions::builder()
                    .name(Some("idx_credit_notes_status_created".to_string()))
                    .build(),
            )
            .build(),
    ];

    if let Err(e) = db
        .collection::<mongodb::bson::Document>("credit_notes")
        .create_indexes(credit_note_indexes)
        .await
    {
        warn!("Failed to create analytics indexes on credit_notes: {e}");
    }

    // 3. Repairs indexes
    let repair_indexes = vec![
        IndexModel::builder()
            .keys(doc! { "status": 1, "technician_key": 1, "completed_at": -1 })
            .options(
                IndexOptions::builder()
                    .name(Some("idx_repairs_tech_status_completed".to_string()))
                    .build(),
            )
            .build(),
    ];

    if let Err(e) = db
        .collection::<mongodb::bson::Document>("repairs")
        .create_indexes(repair_indexes)
        .await
    {
        warn!("Failed to create analytics indexes on repairs: {e}");
    }

    // 4. Print jobs indexes
    let print_job_indexes = vec![
        IndexModel::builder()
            .keys(doc! { "status": 1, "operator_key": 1, "completed_at": -1 })
            .options(
                IndexOptions::builder()
                    .name(Some("idx_print_jobs_op_status_completed".to_string()))
                    .build(),
            )
            .build(),
    ];

    if let Err(e) = db
        .collection::<mongodb::bson::Document>("print_jobs")
        .create_indexes(print_job_indexes)
        .await
    {
        warn!("Failed to create analytics indexes on print_jobs: {e}");
    }

    info!("Analytics database indexes verified");
}
