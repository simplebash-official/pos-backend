use serde::{Deserialize, Serialize};

use crate::{
    calculations::share_basis_points,
    error::{CalcError, CalcResult},
};

const MAX_PARTS: usize = 200;
const MAX_CENTS: i64 = 10_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct SharePart {
    /// Echoed back so the screen matches replies by name, never by position.
    pub key: String,
    pub cents: i64,
    /// What this part is a share of. Absent: the total of all the parts sent.
    pub whole_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcSharesRequest {
    pub parts: Vec<SharePart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct ShareLine {
    pub key: String,
    /// Percent x 100, between 0 and 10000.
    pub basis_points: i64,
    /// Whole percent, rounded half up, between 0 and 100.
    pub percent: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcSharesReply {
    /// The total of all the parts sent.
    pub total_cents: i64,
    pub parts: Vec<ShareLine>,
}

pub fn compute(request: &CalcSharesRequest) -> CalcResult<CalcSharesReply> {
    if request.parts.len() > MAX_PARTS {
        return Err(CalcError::validation("Too many parts"));
    }
    let mut total = 0_i64;
    for part in &request.parts {
        let whole = part.whole_cents.unwrap_or(0);
        if part.cents.abs() > MAX_CENTS || whole.abs() > MAX_CENTS {
            return Err(CalcError::validation("An amount is out of range"));
        }
        total += part.cents.max(0);
    }
    let parts = request
        .parts
        .iter()
        .map(|part| {
            let basis_points = share_basis_points(part.cents, part.whole_cents.unwrap_or(total));
            ShareLine {
                key: part.key.clone(),
                basis_points,
                percent: (basis_points + 50) / 100,
            }
        })
        .collect();
    Ok(CalcSharesReply {
        total_cents: total,
        parts,
    })
}
