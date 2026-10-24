// Money reports for the owner: profit and loss, costs by category and month, cash flow per account
// and what is still to pay. Read-only; nothing here is stored. It reads the sales, the expense and
// income ledgers, the cash movements and the supplier balances through their own services.
pub mod routes;
pub mod service;
