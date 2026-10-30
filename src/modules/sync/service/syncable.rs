/// The resources a client may sync, paired with the collection each reads
/// from. `subcategories` is deliberately absent: a subcategory is not an
/// independent entity to the client, it is a field on the category it
/// belongs to, and `hydrate` folds it in there (see
/// `inventory::service::category::hydrate_sync_documents`).
pub(super) const SYNCABLE: &[(&str, &str)] = &[
    ("products", "products"),
    ("categories", "categories"),
    ("suppliers", "suppliers"),
    ("supplierProducts", "supplier_products"),
    ("purchases", "purchases"),
    ("stockMovements", "stock_movements"),
    ("stockBatches", "stock_batches"),
    ("customers", "customers"),
    ("employees", "employees"),
    ("repairs", "repairs"),
    ("printJobs", "print_jobs"),
    ("invoices", "invoices"),
    ("payments", "payments"),
    ("creditNotes", "credit_notes"),
    ("productSerials", "product_serials"),
];
