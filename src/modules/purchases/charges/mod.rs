// The money maths of a receipt's extra charges, with no I/O: how a charge is spread over the
// receipt's lines, how a line's landed cost is split into stock batches, and what a receipt owes
// the supplier. Everything is integer cents and exact: shares add up to the charge to the cent,
// and the batches of a line add up to its landed total to the cent. The frontend has a twin of
// the spreading and splitting (`features/purchases/lib/landedCost.ts`) that is tested against the
// same vectors, so the form previews exactly what the server will save.

mod allocate;
mod batch_cost;
mod charge_figure;
mod charge_totals;
mod charge_totals_fn;
mod owed_total;
mod rounded_unit;
mod signed_amount;
mod split_cost;
#[cfg(test)]
mod tests;
mod weights;

pub use allocate::allocate;
pub use batch_cost::BatchCost;
pub use charge_figure::ChargeFigure;
pub use charge_totals::ChargeTotals;
pub use charge_totals_fn::charge_totals;
pub use owed_total::owed_total;
pub use rounded_unit::rounded_unit;
pub use signed_amount::signed_amount;
pub use split_cost::split_cost;
pub use weights::weights;
