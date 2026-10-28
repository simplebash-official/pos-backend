// How big each part is next to its whole, for the progress bars and split bars. Pure; the rule is
// `pos_calc_core::shares`.

use crate::core::error::AppResult;

pub use pos_calc_core::shares::{CalcSharesReply, CalcSharesRequest, ShareLine, SharePart};

pub fn compute(request: &CalcSharesRequest) -> AppResult<CalcSharesReply> {
    Ok(pos_calc_core::shares::compute(request)?)
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
