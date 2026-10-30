//! The money maths of a receipt's extra charges, with no I/O: how a charge is spread over the
//! receipt's lines, how a line's landed cost is split into stock batches, and what a receipt owes
//! the supplier. Everything is integer cents and exact: shares add up to the charge to the cent,
//! and the batches of a line add up to its landed total to the cent. The frontend has a twin of
//! the spreading and splitting (`features/purchases/lib/landedCost.ts`) that is tested against the
//! same vectors, so the form previews exactly what the server will save.

mod purchases_charges;
#[cfg(test)]
mod tests;

pub use purchases_charges::BatchCost;
pub use purchases_charges::ChargeFigure;
pub use purchases_charges::ChargeTotals;
pub use purchases_charges::allocate;
pub use purchases_charges::charge_totals;
pub use purchases_charges::owed_total;
pub use purchases_charges::rounded_unit;
pub use purchases_charges::signed_amount;
pub use purchases_charges::split_cost;
pub use purchases_charges::weights;
