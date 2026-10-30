/// One stock batch of a line: how many units and what each costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchCost {
    pub quantity: i64,
    pub unit_cost_cents: i64,
}
