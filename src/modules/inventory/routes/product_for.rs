use crate::core::middleware::auth::CurrentUser;

use crate::domain::inventory::Product;
use crate::domain::users::Role;

/// What the shop paid for a product is Admin-only: every other caller gets
/// the product with `costPriceCents` absent. Applied here, on the way out,
/// because the service layer and sync work with the real value.
pub(super) fn product_for(user: &CurrentUser, product: Product) -> Product {
    if user.role == Some(Role::Admin) {
        product
    } else {
        product.without_cost()
    }
}
