// Live calculations for the screens: the client sends what the person has typed so far and gets
// back every figure, toggle counterpart and "what stops Save" for it. Nothing is written (a few
// read the invoice, drawer or category they depend on);
// the formulas live in the domain engines (`purchases::charges`, `purchases::pricing`, ...) and
// this module only shapes requests and replies. The save paths call the same engines, so a
// preview can never disagree with what is stored.
pub mod cash;
pub mod checkout;
pub mod credit_note;
pub mod earnings;
pub mod invoice_balance;
pub mod quotation;
pub mod receipt;
pub mod routes;
pub mod settlement;
pub mod shares;
pub mod shift_totals;
