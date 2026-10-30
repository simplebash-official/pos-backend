use super::other_name::OTHER_NAME;
use crate::domain::cash_categories::category_key;
use crate::domain::sessions::CashMovementType;

/// Key of the type's "Other" category, where one-off labels and migrated movements are filed.
#[must_use]
pub fn other_category_key(movement_type: CashMovementType) -> String {
    category_key(movement_type, OTHER_NAME)
}
