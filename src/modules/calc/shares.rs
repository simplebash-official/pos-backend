// How big each part is next to its whole, for the progress bars and split bars. Pure; the rule is
// `core::calculations::share_basis_points`, so every bar in the app rounds the same way.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::core::{
    calculations::share_basis_points,
    error::{AppError, AppResult},
};

const MAX_PARTS: usize = 200;
const MAX_CENTS: i64 = 10_000_000_000_000;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharePart {
    /// Echoed back so the screen matches replies by name, never by position.
    pub key: String,
    pub cents: i64,
    /// What this part is a share of. Absent: the total of all the parts sent.
    pub whole_cents: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSharesRequest {
    pub parts: Vec<SharePart>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareLine {
    pub key: String,
    /// Percent x 100, between 0 and 10000.
    pub basis_points: i64,
    /// Whole percent, rounded half up, between 0 and 100.
    pub percent: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcSharesReply {
    /// The total of all the parts sent.
    pub total_cents: i64,
    pub parts: Vec<ShareLine>,
}

pub fn compute(request: &CalcSharesRequest) -> AppResult<CalcSharesReply> {
    if request.parts.len() > MAX_PARTS {
        return Err(AppError::validation("Too many parts"));
    }
    let mut total = 0_i64;
    for part in &request.parts {
        let whole = part.whole_cents.unwrap_or(0);
        if part.cents.abs() > MAX_CENTS || whole.abs() > MAX_CENTS {
            return Err(AppError::validation("An amount is out of range"));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn part(key: &str, cents: i64, whole: Option<i64>) -> SharePart {
        SharePart {
            key: key.into(),
            cents,
            whole_cents: whole,
        }
    }

    #[test]
    fn parts_default_to_a_share_of_their_own_total() {
        let reply = compute(&CalcSharesRequest {
            parts: vec![
                part("cash", 300, None),
                part("card", 100, None),
                part("none", 0, None),
            ],
        })
        .unwrap();
        assert_eq!(reply.total_cents, 400);
        assert_eq!(reply.parts[0].percent, 75);
        assert_eq!(reply.parts[1].basis_points, 2_500);
        assert_eq!(reply.parts[2].percent, 0);
    }

    #[test]
    fn a_part_given_its_own_whole_is_capped_at_full() {
        let reply = compute(&CalcSharesRequest {
            parts: vec![part("bill", 15_000, Some(10_000))],
        })
        .unwrap();
        assert_eq!(reply.parts[0].percent, 100);
    }
}
