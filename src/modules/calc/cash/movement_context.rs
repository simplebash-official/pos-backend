/// What the handler reads for the request.
#[derive(Debug)]
pub struct MovementContext {
    pub cash_in_drawer_cents: i64,
    pub category_limit_cents: Option<i64>,
    pub shop_limit_cents: Option<i64>,
    pub period_already_paid: bool,
}
