// What a product earns and the selling price a wanted margin asks for, with no I/O. Integer cents
// throughout; the lot's cost is the landed total (price plus its share of extra charges), never a
// per-unit cost rounded first. This is the only copy of these formulas: the Receive Stock screen
// asks the server for them on every change (`modules::calc`).

mod line_margin;
mod line_margin_fn;
mod narrow;
mod per_mille;
mod round_half_up;
mod suggested_price;
#[cfg(test)]
mod tests;

pub use line_margin::LineMargin;
pub use line_margin_fn::line_margin;
pub use suggested_price::suggested_price;
