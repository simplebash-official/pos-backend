pub const ACCOUNT_OPENING: &str = "aopen";

pub const ADVANCE_RECOVERY: &str = "advr";

pub const API_KEY: &str = "ak";

pub const ATTACHMENT: &str = "att";

/// Account opening balances use fixed keys `aopen_<kind>` so the same account set up on two devices is one row.
/// Balance openings use fixed keys `bopen_<subject key>`.
pub const BALANCE_OPENING: &str = "bopen";

pub const CASH_CATEGORY: &str = "ccat";

pub const CASH_MOVEMENT: &str = "cshm";

/// The shop's drawer rules are one row with the fixed key `cset_shop`.
pub const CASH_SETTINGS_KEY: &str = "cset_shop";

pub const CATEGORY: &str = "cat";

pub const CREDIT_NOTE: &str = "cn";

pub const CUSTOMER: &str = "cust";

pub const EMPLOYEE: &str = "emp";

pub const EXCHANGE: &str = "exg";

pub const EXPENSE: &str = "exp";

pub const EXPENSE_PAYMENT: &str = "expp";

pub const GENERATED_DOCUMENT: &str = "doc";

pub const IMPORT_BATCH: &str = "imp";

pub const INCOME: &str = "inc";

pub const INVOICE: &str = "inv";

pub const LABEL_PRINT_JOB: &str = "lpj";

pub const LABEL_TEMPLATE: &str = "ltpl";

pub const LOGIN_SESSION: &str = "lgs";

/// Key of the batch the migration to stock batches gives every product for the
/// stock (and price) it held before batches existed. Deterministic, so the
/// desktop's SQLite migration and the cloud's Mongo migration create the same
/// row and sync merges them instead of duplicating the stock.
#[must_use]
pub fn opening_batch_key(product_key: &str) -> String {
    format!("{STOCK_BATCH}_open_{product_key}")
}

pub const PAYEE: &str = "pye";

pub const PAYMENT: &str = "pay";

pub const PRINT_JOB: &str = "prn";

// Standard entity ID prefixes used for generating prefixed NanoID model keys.
pub const PRODUCT: &str = "prod";

pub const PRODUCT_SERIAL: &str = "psn";

pub const PURCHASE: &str = "pur";

pub const PURCHASE_RECEIPT: &str = "prc";

pub const QUOTATION: &str = "quo";

pub const RECEIPT_CHARGE: &str = "rchg";

pub const RECEIPT_SETTLEMENT: &str = "rset";

pub const RECURRING_EXPENSE: &str = "rexp";

pub const REGISTER_SESSION: &str = "sess";

pub const REPAIR: &str = "rep";

pub const SEQUENCE_BLOCK: &str = "seq";

pub const STOCK_BATCH: &str = "stb";

pub const STOCK_MOVEMENT: &str = "sm";

pub const SUBCATEGORY: &str = "subcat";

pub const SUPPLIER: &str = "sup";

pub const SUPPLIER_CREDIT: &str = "scr";

pub const SUPPLIER_PRODUCT: &str = "splink";

pub const TENANT: &str = "tnt";

pub const USER: &str = "usr";

pub const WARRANTY: &str = "wty";

pub const WARRANTY_EVENT: &str = "wev";

pub const WARRANTY_CLAIM: &str = "wcl";
