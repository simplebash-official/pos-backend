use crate::domain::cash_categories::PayeeType;

/// Who a payee key refers to, resolved from the module that owns it.
#[derive(Debug)]
pub(crate) struct ResolvedPayee {
    pub payee_type: PayeeType,
    pub name: String,
}
