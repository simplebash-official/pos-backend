use crate::core::constants::prefixes;
use crate::domain::accounts::AccountKind;

pub(super) fn opening_key(kind: AccountKind) -> String {
    format!("{}_{}", prefixes::ACCOUNT_OPENING, kind.as_str())
}
