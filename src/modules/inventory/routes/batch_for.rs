use crate::core::middleware::auth::CurrentUser;

use crate::domain::inventory::StockBatch;
use crate::domain::users::Role;

/// A batch's cost is Admin-only, like a product's (see `product_for`).
pub(super) fn batch_for(user: &CurrentUser, batch: StockBatch) -> StockBatch {
    if user.role == Some(Role::Admin) {
        batch
    } else {
        batch.without_cost()
    }
}
