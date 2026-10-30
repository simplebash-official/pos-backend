/// A single time-bucket granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Day,
    Week,
    Month,
    Year,
}

impl Granularity {
    #[must_use]
    pub fn as_mongo_unit(self) -> &'static str {
        match self {
            Granularity::Day => "day",
            Granularity::Week => "week",
            Granularity::Month => "month",
            Granularity::Year => "year",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Granularity::Day => "Daily",
            Granularity::Week => "Weekly",
            Granularity::Month => "Monthly",
            Granularity::Year => "Yearly",
        }
    }
}
