//! What a product earns and the selling price a wanted margin asks for, with no I/O. Integer cents
//! throughout; the lot's cost is the landed total (price plus its share of extra charges), never a
//! per-unit cost rounded first. This is the only copy of these formulas: the Receive Stock screen
//! asks the server for them on every change (`modules::calc`).

mod purchases_pricing;
#[cfg(test)]
mod tests;

pub use purchases_pricing::LineMargin;
pub use purchases_pricing::line_margin;
pub use purchases_pricing::suggested_price;
