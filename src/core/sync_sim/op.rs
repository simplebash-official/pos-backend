/// One step of a scenario. `sel` fields pick among the rows visible to the
/// acting device at run time.
#[derive(Debug, Clone)]
pub enum Op {
    NewProduct {
        dev: usize,
        price: i64,
    },
    NewCustomer {
        dev: usize,
    },
    PriceEdit {
        dev: usize,
        sel: u64,
        price: i64,
    },
    DeleteProduct {
        dev: usize,
        sel: u64,
    },
    StockAdjust {
        dev: usize,
        sel: u64,
        delta: i64,
    },
    Sale {
        dev: usize,
        sel_product: u64,
        sel_customer: u64,
        qty: i64,
        credit: bool,
        deposit_percent: i64,
    },
    Payment {
        dev: usize,
        sel_invoice: u64,
        amount: i64,
    },
    Return {
        dev: usize,
        sel_invoice: u64,
        qty: i64,
    },
    VoidInvoice {
        dev: usize,
        sel_invoice: u64,
    },
    NoteInvoice {
        dev: usize,
        sel_invoice: u64,
        note: u64,
    },
    Offline {
        dev: usize,
    },
    Online {
        dev: usize,
    },
    Skew {
        dev: usize,
        ms: i64,
        known: bool,
    },
    Sync {
        dev: usize,
    },
}
