-- SQLite schema for simplebash-pos backend
-- All tables maintain custom unique key, timestamps, version, and soft deletion.

CREATE TABLE IF NOT EXISTS products (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    sku TEXT NOT NULL UNIQUE,
    barcode TEXT,
    barcode_source TEXT,
    name TEXT NOT NULL,
    category_key TEXT NOT NULL,
    subcategory_key TEXT NOT NULL,
    cost_price_cents INTEGER NOT NULL,
    selling_price_cents INTEGER NOT NULL,
    stock_quantity INTEGER NOT NULL DEFAULT 0,
    min_stock_threshold INTEGER NOT NULL DEFAULT 0,
    is_serialized INTEGER NOT NULL DEFAULT 0,
    warranty_months INTEGER,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_products_category_key ON products(category_key);
CREATE INDEX IF NOT EXISTS idx_products_subcategory_key ON products(subcategory_key);
CREATE INDEX IF NOT EXISTS idx_products_barcode ON products(barcode);
CREATE INDEX IF NOT EXISTS idx_products_sync ON products(updated_at, key);

CREATE TABLE IF NOT EXISTS categories (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    name TEXT NOT NULL UNIQUE,
    icon TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT '',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_categories_sync ON categories(updated_at, key);

CREATE TABLE IF NOT EXISTS subcategories (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    category_key TEXT NOT NULL,
    name TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_subcategories_category_key ON subcategories(category_key);
CREATE INDEX IF NOT EXISTS idx_subcategories_sync ON subcategories(updated_at, key);

CREATE TABLE IF NOT EXISTS suppliers (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    name TEXT NOT NULL UNIQUE,
    contact_person TEXT NOT NULL,
    primary_phone TEXT NOT NULL,
    secondary_phone TEXT,
    email TEXT,
    address TEXT NOT NULL,
    supplied_categories TEXT NOT NULL DEFAULT '[]',
    notes TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_suppliers_sync ON suppliers(updated_at, key);

CREATE TABLE IF NOT EXISTS supplier_products (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    supplier_key TEXT NOT NULL,
    product_key TEXT NOT NULL,
    cost_price_cents INTEGER,
    notes TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_sp_supplier_key ON supplier_products(supplier_key);
CREATE INDEX IF NOT EXISTS idx_sp_product_key ON supplier_products(product_key);
CREATE INDEX IF NOT EXISTS idx_sp_sync ON supplier_products(updated_at, key);

CREATE TABLE IF NOT EXISTS purchases (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    supplier_key TEXT NOT NULL,
    product_key TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    unit_cost_cents INTEGER NOT NULL,
    date TEXT NOT NULL,
    reference_no TEXT,
    notes TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_purchases_supplier_key ON purchases(supplier_key);
CREATE INDEX IF NOT EXISTS idx_purchases_product_key ON purchases(product_key);
CREATE INDEX IF NOT EXISTS idx_purchases_sync ON purchases(updated_at, key);

CREATE TABLE IF NOT EXISTS stock_movements (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    quantity_delta INTEGER NOT NULL,
    movement_type TEXT NOT NULL,
    reference_id TEXT,
    note TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_stock_movements_product_id ON stock_movements(product_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_sync ON stock_movements(updated_at, key);

CREATE TABLE IF NOT EXISTS customers (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    contact_person TEXT,
    primary_phone TEXT NOT NULL,
    secondary_phone TEXT,
    email TEXT,
    address TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    notes TEXT,
    outstanding_balance_cents INTEGER NOT NULL DEFAULT 0,
    total_purchases_cents INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_customers_primary_phone ON customers(primary_phone);
CREATE INDEX IF NOT EXISTS idx_customers_email ON customers(email);
CREATE INDEX IF NOT EXISTS idx_customers_sync ON customers(updated_at, key);

CREATE TABLE IF NOT EXISTS employees (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    phone TEXT NOT NULL,
    nic_or_id TEXT,
    role TEXT NOT NULL,
    default_split_type TEXT NOT NULL,
    default_split_value REAL NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    notes TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_employees_sync ON employees(updated_at, key);

CREATE TABLE IF NOT EXISTS repairs (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    ticket_number TEXT NOT NULL UNIQUE,
    customer_key TEXT,
    customer_name TEXT NOT NULL,
    customer_phone TEXT NOT NULL,
    device_model TEXT NOT NULL,
    serial_number TEXT,
    issue_description TEXT NOT NULL,
    promised_ready_at TEXT,
    status TEXT NOT NULL,
    estimated_cost_cents INTEGER,
    material_cost_cents INTEGER,
    assigned_employee_id TEXT,
    assigned_employee_name TEXT,
    split_type TEXT,
    split_value REAL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_repairs_customer_key ON repairs(customer_key);
CREATE INDEX IF NOT EXISTS idx_repairs_status ON repairs(status);
CREATE INDEX IF NOT EXISTS idx_repairs_sync ON repairs(updated_at, key);

CREATE TABLE IF NOT EXISTS print_jobs (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    ticket_number TEXT NOT NULL UNIQUE,
    customer_key TEXT,
    customer_name TEXT NOT NULL,
    customer_phone TEXT,
    job_type TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 1,
    promised_ready_at TEXT,
    status TEXT NOT NULL,
    estimated_cost_cents INTEGER NOT NULL,
    material_cost_cents INTEGER,
    assigned_employee_id TEXT,
    assigned_employee_name TEXT,
    split_type TEXT,
    split_value REAL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_print_jobs_customer_key ON print_jobs(customer_key);
CREATE INDEX IF NOT EXISTS idx_print_jobs_status ON print_jobs(status);
CREATE INDEX IF NOT EXISTS idx_print_jobs_sync ON print_jobs(updated_at, key);

CREATE TABLE IF NOT EXISTS invoices (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    invoice_number TEXT NOT NULL UNIQUE,
    customer_key TEXT,
    customer_name_snapshot TEXT,
    customer_phone_snapshot TEXT,
    customer_address_snapshot TEXT,
    cashier_id TEXT NOT NULL,
    cashier_name_snapshot TEXT NOT NULL,
    items JSON NOT NULL,
    subtotal_cents INTEGER NOT NULL,
    discount_type TEXT NOT NULL DEFAULT 'fixed',
    discount_value REAL NOT NULL DEFAULT 0.0,
    discount_cents INTEGER NOT NULL DEFAULT 0,
    total_cents INTEGER NOT NULL,
    payment_method TEXT NOT NULL,
    split_payments JSON,
    is_credit INTEGER NOT NULL DEFAULT 0,
    amount_received_cents INTEGER,
    change_due_cents INTEGER,
    due_date TEXT,
    card_last4 TEXT,
    card_ref TEXT,
    online_ref TEXT,
    online_note TEXT,
    status TEXT NOT NULL,
    notes TEXT,
    shop_profile_snapshot JSON NOT NULL,
    warranty_terms_snapshot TEXT,
    document_selection TEXT,
    voided_at TEXT,
    voided_by TEXT,
    voided_reason TEXT,
    closed_at TEXT,
    closed_by TEXT,
    refunded_cents INTEGER NOT NULL DEFAULT 0,
    credit_note_count INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_invoices_customer_key ON invoices(customer_key);
CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(status);
CREATE INDEX IF NOT EXISTS idx_invoices_created_at ON invoices(created_at);
CREATE INDEX IF NOT EXISTS idx_invoices_sync ON invoices(updated_at, key);

CREATE TABLE IF NOT EXISTS payments (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    invoice_key TEXT NOT NULL,
    amount_cents INTEGER NOT NULL,
    payment_method TEXT NOT NULL,
    notes TEXT,
    recorded_by_user_id TEXT NOT NULL,
    recorded_by_name_snapshot TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_payments_invoice_key ON payments(invoice_key);
CREATE INDEX IF NOT EXISTS idx_payments_created_at ON payments(created_at);
CREATE INDEX IF NOT EXISTS idx_payments_sync ON payments(updated_at, key);

CREATE TABLE IF NOT EXISTS credit_notes (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    credit_note_number TEXT NOT NULL UNIQUE,
    invoice_key TEXT,
    invoice_number TEXT,
    no_receipt INTEGER NOT NULL DEFAULT 0,
    customer_key TEXT,
    customer_name_snapshot TEXT,
    cashier_id TEXT NOT NULL,
    cashier_name_snapshot TEXT NOT NULL,
    returned_items JSON NOT NULL,
    exchange_items JSON NOT NULL DEFAULT '[]',
    exchange_reference TEXT,
    return_subtotal_cents INTEGER NOT NULL,
    exchange_subtotal_cents INTEGER NOT NULL,
    net_refund_cents INTEGER NOT NULL,
    refund_cash_cents INTEGER NOT NULL DEFAULT 0,
    balance_reduction_cents INTEGER NOT NULL DEFAULT 0,
    refund_breakdown JSON NOT NULL DEFAULT '[]',
    refund_payment_keys JSON NOT NULL DEFAULT '[]',
    status TEXT NOT NULL,
    is_manager_override INTEGER NOT NULL DEFAULT 0,
    override_approved_by TEXT,
    override_reason TEXT,
    notes TEXT,
    voided_at TEXT,
    voided_by TEXT,
    voided_reason TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE INDEX IF NOT EXISTS idx_credit_notes_invoice_key ON credit_notes(invoice_key);
CREATE INDEX IF NOT EXISTS idx_credit_notes_sync ON credit_notes(updated_at, key);

CREATE TABLE IF NOT EXISTS product_serials (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    product_key TEXT NOT NULL,
    serial_number TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    invoice_key TEXT,
    sold_at TEXT,
    warranty_months INTEGER,
    warranty_expires_at TEXT,
    credit_note_key TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ps_product_key_status ON product_serials(product_key, status);
CREATE INDEX IF NOT EXISTS idx_ps_sync ON product_serials(updated_at, key);

CREATE TABLE IF NOT EXISTS users (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL,
    employee_key TEXT,
    preferences_json TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

CREATE TABLE IF NOT EXISTS login_sessions (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    user_key TEXT NOT NULL,
    name_at_login TEXT NOT NULL,
    username_at_login TEXT NOT NULL,
    role_at_login TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sku_counters (
    prefix TEXT PRIMARY KEY,
    seq INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS barcode_counters (
    prefix TEXT PRIMARY KEY,
    seq INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS sequence_counters (
    name TEXT PRIMARY KEY,
    prefix TEXT NOT NULL,
    padding INTEGER NOT NULL DEFAULT 6,
    next_val INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sequence_blocks (
    key TEXT PRIMARY KEY,
    counter_name TEXT NOT NULL,
    device_id TEXT,
    start_seq INTEGER NOT NULL,
    end_seq INTEGER NOT NULL,
    current_seq INTEGER NOT NULL,
    reserved_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    key TEXT NOT NULL,
    user_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    response_status INTEGER,
    response_body TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (key, user_id)
);

CREATE TABLE IF NOT EXISTS import_batches (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_type TEXT NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    target TEXT NOT NULL,
    status TEXT NOT NULL,
    total_rows INTEGER NOT NULL DEFAULT 0,
    processed_rows INTEGER NOT NULL DEFAULT 0,
    successful_rows INTEGER NOT NULL DEFAULT 0,
    failed_rows INTEGER NOT NULL DEFAULT 0,
    errors JSON,
    created_by_user_key TEXT,
    created_by_user_name TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS generated_documents (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    document_type TEXT NOT NULL,
    template_name TEXT NOT NULL,
    template_key TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size_bytes INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_generated_documents_lookup ON generated_documents(entity_key, document_type, created_at DESC);

CREATE TABLE IF NOT EXISTS api_keys (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    hashed_key TEXT NOT NULL,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS system_installations (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    installation_id TEXT NOT NULL UNIQUE,
    app_version TEXT NOT NULL,
    platform TEXT NOT NULL,
    installed_at TEXT NOT NULL,
    setup_completed INTEGER NOT NULL DEFAULT 0,
    setup_completed_at TEXT,
    sample_data_loaded INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_system_installations_id ON system_installations(installation_id);


-- ---------------------------------------------------------------------------
-- Sync v2 (device side). Local-only bookkeeping: none of these tables is ever
-- synced, and backup export/import skips every `sync_*` table.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    device_id TEXT NOT NULL,
    tenant_id TEXT,
    linked INTEGER NOT NULL DEFAULT 0,
    applying INTEGER NOT NULL DEFAULT 0,
    cloud_cursor INTEGER NOT NULL DEFAULT 0,
    last_pushed_outbox_seq INTEGER NOT NULL DEFAULT 0,
    clock_offset_ms INTEGER NOT NULL DEFAULT 0,
    capture_enabled INTEGER NOT NULL DEFAULT 0,
    bootstrap_active INTEGER NOT NULL DEFAULT 0,
    outbox_epoch TEXT
);

CREATE TABLE IF NOT EXISTS sync_outbox (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    resource TEXT NOT NULL,
    key TEXT NOT NULL,
    op TEXT NOT NULL CHECK (op IN ('upsert', 'delete')),
    enqueued_at TEXT NOT NULL,
    UNIQUE (resource, key)
);

CREATE TABLE IF NOT EXISTS sync_row_meta (
    resource TEXT NOT NULL,
    key TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    device_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    PRIMARY KEY (resource, key)
);

CREATE TABLE IF NOT EXISTS sync_conflicts (
    key TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    resource TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    detail JSON NOT NULL,
    detected_at TEXT NOT NULL,
    resolved_at TEXT,
    resolution TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_conflicts_open ON sync_conflicts(resolved_at, detected_at);

CREATE TABLE IF NOT EXISTS sync_number_blocks (
    name TEXT NOT NULL,
    prefix TEXT NOT NULL,
    padding INTEGER NOT NULL,
    start_seq INTEGER NOT NULL,
    end_seq INTEGER NOT NULL,
    next_seq INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (name, start_seq)
);

CREATE TABLE IF NOT EXISTS shop_profiles (
    key TEXT PRIMARY KEY,
    id TEXT NOT NULL,
    legal_name TEXT NOT NULL DEFAULT '',
    trading_name TEXT NOT NULL DEFAULT '',
    address_lines TEXT NOT NULL DEFAULT '[]',
    primary_phone TEXT NOT NULL DEFAULT '',
    secondary_phone TEXT NOT NULL DEFAULT '',
    email TEXT NOT NULL DEFAULT '',
    website TEXT NOT NULL DEFAULT '',
    business_reg_no TEXT NOT NULL DEFAULT '',
    logo_base64 TEXT NOT NULL DEFAULT '',
    bank_name TEXT NOT NULL DEFAULT '',
    bank_branch TEXT NOT NULL DEFAULT '',
    account_name TEXT NOT NULL DEFAULT '',
    account_number TEXT NOT NULL DEFAULT '',
    default_warranty_text TEXT NOT NULL DEFAULT '',
    default_footer_text TEXT NOT NULL DEFAULT '',
    receipt_footer_text TEXT NOT NULL DEFAULT '',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    updated_by_device TEXT
);

