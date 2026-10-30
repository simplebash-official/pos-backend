use super::line_hint::LineHint;

use crate::domain::billing::InvoiceItem;

/// Finds the invoice line a returned item refers to: product key (narrowed by
/// `hint` when the product is on more than one line), then source ticket key,
/// then case-insensitive name; a sole line with a sole returned item always
/// matches. Shared with sync's derived-field recompute so a replicated credit
/// note updates the same line a local one would.
pub(crate) fn line_match(
    items: &[InvoiceItem],
    product_key: Option<&str>,
    source_ticket_key: Option<&str>,
    name: Option<&str>,
    hint: LineHint,
    sole_line_and_sole_return: bool,
) -> Option<usize> {
    items.iter().position(|item| {
        if let (Some(req_pk), Some(item_pk)) = (product_key, item.product_key.as_deref())
            && req_pk == item_pk
        {
            let price_matches = hint
                .unit_price_cents
                .is_none_or(|price| price == item.unit_price_cents);
            let batches_match = hint.batch_keys.iter().all(|key| {
                item.batch_allocations
                    .iter()
                    .any(|allocation| allocation.batch_key == *key)
            });
            return price_matches && batches_match;
        }
        if let (Some(req_stk), Some(item_stk)) =
            (source_ticket_key, item.source_ticket_key.as_deref())
            && req_stk == item_stk
        {
            return true;
        }
        if let Some(req_name) = name
            && req_name.eq_ignore_ascii_case(&item.name)
        {
            return true;
        }
        sole_line_and_sole_return
    })
}
