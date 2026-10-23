//! Standard entity ID prefixes used for generating prefixed NanoID model keys.

pub const PRODUCT: &str = "prod";
pub const CATEGORY: &str = "cat";
pub const SUBCATEGORY: &str = "subcat";
pub const STOCK_MOVEMENT: &str = "sm";
pub const API_KEY: &str = "ak";
pub const SUPPLIER: &str = "sup";
pub const SUPPLIER_PRODUCT: &str = "splink";
pub const PURCHASE: &str = "pur";
pub const USER: &str = "usr";
pub const LOGIN_SESSION: &str = "lgs";
pub const SEQUENCE_BLOCK: &str = "seq";
pub const CUSTOMER: &str = "cust";
pub const GENERATED_DOCUMENT: &str = "doc";
pub const INVOICE: &str = "inv";
pub const PAYMENT: &str = "pay";
pub const REPAIR: &str = "rep";
pub const PRINT_JOB: &str = "prn";
pub const CREDIT_NOTE: &str = "cn";
pub const EXCHANGE: &str = "exg";
pub const PRODUCT_SERIAL: &str = "psn";
pub const EMPLOYEE: &str = "emp";
pub const IMPORT_BATCH: &str = "imp";
pub const TENANT: &str = "tnt";
pub const QUOTATION: &str = "quo";
pub const STOCK_BATCH: &str = "stb";
pub const LABEL_TEMPLATE: &str = "ltpl";
pub const LABEL_PRINT_JOB: &str = "lpj";
pub const REGISTER_SESSION: &str = "sess";
pub const CASH_MOVEMENT: &str = "cshm";
pub const CASH_CATEGORY: &str = "ccat";
pub const PAYEE: &str = "pye";
pub const EXPENSE: &str = "exp";
pub const RECURRING_EXPENSE: &str = "rexp";
pub const EXPENSE_PAYMENT: &str = "expp";
pub const INCOME: &str = "inc";
/// Account opening balances use fixed keys `aopen_<kind>` so the same account set up on two devices is one row.
/// Balance openings use fixed keys `bopen_<subject key>`.
pub const BALANCE_OPENING: &str = "bopen";
/// The shop's drawer rules are one row with the fixed key `cset_shop`.
pub const CASH_SETTINGS_KEY: &str = "cset_shop";
pub const ATTACHMENT: &str = "att";
pub const ADVANCE_RECOVERY: &str = "advr";
pub const ACCOUNT_OPENING: &str = "aopen";

/// Key of the batch the migration to stock batches gives every product for the
/// stock (and price) it held before batches existed. Deterministic, so the
/// desktop's SQLite migration and the cloud's Mongo migration create the same
/// row and sync merges them instead of duplicating the stock.
pub fn opening_batch_key(product_key: &str) -> String {
    format!("{STOCK_BATCH}_open_{product_key}")
}
