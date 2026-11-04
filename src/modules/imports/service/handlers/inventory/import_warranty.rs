//! Reads an import row's warranty cells: `warranty` ("12 months", "2 years", "lifetime",
//! "none"; empty means no warranty), `warrantyType` ("shop", "manufacturer", "both", required
//! when covered) and `warrantyTerms` (optional text).

use crate::domain::warranties::{ProductWarranty, WarrantyCoverage, WarrantyType, WarrantyUnit};

/// Reads the coverage cell. An empty cell means no warranty.
///
/// # Errors
///
/// A message naming what the cell should look like.
pub(super) fn parse_coverage(cell: Option<&str>) -> Result<WarrantyCoverage, String> {
    let Some(text) = cell.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(WarrantyCoverage::None);
    };
    let lower = text.to_lowercase();
    match lower.as_str() {
        "none" | "no" | "no warranty" => return Ok(WarrantyCoverage::None),
        "lifetime" => return Ok(WarrantyCoverage::Lifetime),
        _ => {}
    }
    let mut parts = lower.split_whitespace();
    let (Some(count), Some(unit), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!(
            "'{text}' is not a warranty. Write it like '12 months', '2 years', 'lifetime' or 'none'."
        ));
    };
    let value: u32 = count
        .parse()
        .map_err(|_| format!("'{count}' in '{text}' is not a whole number."))?;
    let unit = match unit {
        "day" | "days" => WarrantyUnit::Days,
        "week" | "weeks" => WarrantyUnit::Weeks,
        "month" | "months" => WarrantyUnit::Months,
        "year" | "years" => WarrantyUnit::Years,
        other => {
            return Err(format!(
                "'{other}' is not a warranty unit. Use days, weeks, months or years."
            ));
        }
    };
    let coverage = WarrantyCoverage::Limited { value, unit };
    coverage.validate().map_err(|err| err.to_string())?;
    Ok(coverage)
}

/// Reads the type cell. Empty is `None`.
///
/// # Errors
///
/// A message listing the allowed values.
pub(super) fn parse_type(cell: Option<&str>) -> Result<Option<WarrantyType>, String> {
    let Some(text) = cell.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    match text.to_lowercase().as_str() {
        "shop" => Ok(Some(WarrantyType::Shop)),
        "manufacturer" => Ok(Some(WarrantyType::Manufacturer)),
        "both" => Ok(Some(WarrantyType::Both)),
        _ => Err(format!(
            "'{text}' is not a warranty type. Use shop, manufacturer or both."
        )),
    }
}

/// Builds and checks the whole warranty from the three cells.
///
/// # Errors
///
/// The first problem found, worded for the import report.
pub(super) fn parse_row_warranty(
    coverage: Option<&str>,
    warranty_type: Option<&str>,
    terms: Option<&str>,
) -> Result<ProductWarranty, String> {
    ProductWarranty {
        coverage: parse_coverage(coverage)?,
        warranty_type: parse_type(warranty_type)?,
        terms: terms
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string),
    }
    .validated()
    .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_cells_are_read_strictly() {
        assert_eq!(parse_coverage(None), Ok(WarrantyCoverage::None));
        assert_eq!(parse_coverage(Some("  ")), Ok(WarrantyCoverage::None));
        assert_eq!(parse_coverage(Some("None")), Ok(WarrantyCoverage::None));
        assert_eq!(
            parse_coverage(Some("Lifetime")),
            Ok(WarrantyCoverage::Lifetime)
        );
        assert_eq!(
            parse_coverage(Some("12 Months")),
            Ok(WarrantyCoverage::Limited {
                value: 12,
                unit: WarrantyUnit::Months
            })
        );
        assert_eq!(
            parse_coverage(Some("1 year")),
            Ok(WarrantyCoverage::Limited {
                value: 1,
                unit: WarrantyUnit::Years
            })
        );
        assert!(
            parse_coverage(Some("12")).is_err(),
            "a bare number has no unit"
        );
        assert!(parse_coverage(Some("12 moons")).is_err());
        assert!(parse_coverage(Some("-3 days")).is_err());
        assert!(parse_coverage(Some("0 days")).is_err());
        assert!(parse_coverage(Some("1 year 2 months")).is_err());
    }

    #[test]
    fn a_covered_row_needs_a_type() {
        assert!(parse_row_warranty(Some("6 months"), None, None).is_err());
        let ok = parse_row_warranty(Some("6 months"), Some("Shop"), Some(" Battery 3 months "))
            .expect("a complete row reads");
        assert_eq!(ok.warranty_type, Some(WarrantyType::Shop));
        assert_eq!(ok.terms.as_deref(), Some("Battery 3 months"));
        assert_eq!(
            parse_row_warranty(None, None, None),
            Ok(ProductWarranty::none())
        );
        assert!(
            parse_row_warranty(None, Some("shop"), None).is_err(),
            "no warranty cannot have a type"
        );
    }
}
