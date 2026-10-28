use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{CalcError, CalcResult};

pub const DENOMINATION_RUPEES: [i64; 10] = [5000, 1000, 500, 100, 50, 20, 10, 5, 2, 1];

const MAX_COUNT: i64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcCashCountRequest {
    /// How many of each note or coin, keyed by its rupee value ("5000", "1000", ... "1").
    #[cfg_attr(feature = "tsify", tsify(type = "Record<string, number>"))]
    pub counts: HashMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CashCountLine {
    /// The rupee value, the key of a stored count.
    pub id: String,
    pub value_cents: i64,
    pub count: i64,
    pub subtotal_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcCashCountReply {
    /// Every denomination, largest first, whether or not any were counted.
    pub lines: Vec<CashCountLine>,
    pub total_cents: i64,
}

pub fn compute_count(request: &CalcCashCountRequest) -> CalcResult<CalcCashCountReply> {
    for (id, count) in &request.counts {
        if !DENOMINATION_RUPEES
            .iter()
            .any(|rupees| rupees.to_string() == *id)
        {
            return Err(CalcError::validation(format!("{id} is not a note or coin")));
        }
        if !(0..=MAX_COUNT).contains(count) {
            return Err(CalcError::validation("A count is out of range"));
        }
    }
    let lines: Vec<CashCountLine> = DENOMINATION_RUPEES
        .iter()
        .map(|rupees| {
            let id = rupees.to_string();
            let count = request.counts.get(&id).copied().unwrap_or(0);
            let value_cents = rupees * 100;
            CashCountLine {
                id,
                value_cents,
                count,
                subtotal_cents: value_cents * count,
            }
        })
        .collect();
    let total_cents = lines.iter().map(|l| l.subtotal_cents).sum();
    Ok(CalcCashCountReply { lines, total_cents })
}
