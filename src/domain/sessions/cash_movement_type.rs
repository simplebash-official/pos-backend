use super::cash_direction::CashDirection;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a drawer cash event is. The type decides which fields the form asks for
/// and which way the cash moves (`direction`); the category refines the purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CashMovementType {
    /// Small daily spends (tea, fuel, stationery).
    PettyExpense,
    /// Shop rent, utilities, licences and instalments.
    RentAndBills,
    /// Salary, advances, bonus and statutory remittances.
    StaffPayment,
    /// Stock and supplier dues paid in cash.
    SupplierPayment,
    /// Owner drawings: equity, never an expense.
    OwnerWithdrawal,
    /// Float, capital or a bank withdrawal put into the drawer.
    AddCash,
    /// Excess cash moved to the safe or the bank.
    CashDrop,
    /// Drawer opened without a sale (audit trail only, no amount).
    NoSale,
}

impl CashMovementType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            CashMovementType::PettyExpense => "petty_expense",
            CashMovementType::RentAndBills => "rent_and_bills",
            CashMovementType::StaffPayment => "staff_payment",
            CashMovementType::SupplierPayment => "supplier_payment",
            CashMovementType::OwnerWithdrawal => "owner_withdrawal",
            CashMovementType::AddCash => "add_cash",
            CashMovementType::CashDrop => "cash_drop",
            CashMovementType::NoSale => "no_sale",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "petty_expense" => Some(CashMovementType::PettyExpense),
            "rent_and_bills" => Some(CashMovementType::RentAndBills),
            "staff_payment" => Some(CashMovementType::StaffPayment),
            "supplier_payment" => Some(CashMovementType::SupplierPayment),
            "owner_withdrawal" => Some(CashMovementType::OwnerWithdrawal),
            "add_cash" => Some(CashMovementType::AddCash),
            "cash_drop" => Some(CashMovementType::CashDrop),
            "no_sale" => Some(CashMovementType::NoSale),
            _ => None,
        }
    }

    /// Which way cash moves, stored on every row so reports total cash in and
    /// out without parsing type names.
    #[must_use]
    pub fn direction(&self) -> CashDirection {
        match self {
            CashMovementType::PettyExpense
            | CashMovementType::RentAndBills
            | CashMovementType::StaffPayment
            | CashMovementType::SupplierPayment
            | CashMovementType::OwnerWithdrawal => CashDirection::Out,
            CashMovementType::AddCash => CashDirection::In,
            CashMovementType::CashDrop | CashMovementType::NoSale => CashDirection::Neutral,
        }
    }
}

impl CashMovementType {
    /// Whether the payment takes cash out of the drawer: every payout and a cash drop.
    #[must_use]
    pub fn takes_cash_out(&self) -> bool {
        self.direction() == CashDirection::Out || *self == CashMovementType::CashDrop
    }

    /// How the movement changes the cash in the drawer: added cash raises it, a payout or drop lowers it.
    #[must_use]
    pub fn drawer_delta_cents(&self, amount_cents: i64) -> i64 {
        match self {
            CashMovementType::AddCash => amount_cents,
            _ if self.takes_cash_out() => -amount_cents,
            _ => 0,
        }
    }

    /// A payout bigger than the cash the drawer holds.
    #[must_use]
    pub fn is_over_drawer(&self, amount_cents: i64, cash_in_drawer_cents: i64) -> bool {
        self.takes_cash_out() && amount_cents > cash_in_drawer_cents
    }

    /// Whether a manager or owner PIN is needed: always for owner drawings and staff pay, above the
    /// category's own limit or the shop-wide one (cash leaving the drawer only), and to go over the drawer.
    #[must_use]
    pub fn needs_manager_pin(
        &self,
        amount_cents: i64,
        category_limit_cents: Option<i64>,
        shop_limit_cents: Option<i64>,
        over_drawer: bool,
    ) -> bool {
        let shop_limit = if self.takes_cash_out() {
            shop_limit_cents
        } else {
            None
        };
        over_drawer
            || matches!(
                self,
                CashMovementType::OwnerWithdrawal | CashMovementType::StaffPayment
            )
            || [category_limit_cents, shop_limit]
                .into_iter()
                .flatten()
                .any(|limit| amount_cents > limit)
    }
}
