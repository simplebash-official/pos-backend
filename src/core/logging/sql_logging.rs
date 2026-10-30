#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlLogging {
    All,
    Slow,
    Off,
}

impl SqlLogging {
    #[must_use]
    pub fn parse(raw: &str) -> SqlLogging {
        match raw.trim().to_ascii_lowercase().as_str() {
            "all" => SqlLogging::All,
            "off" => SqlLogging::Off,
            _ => SqlLogging::Slow,
        }
    }

    pub(super) fn code(self) -> u8 {
        match self {
            SqlLogging::Off => 0,
            SqlLogging::Slow => 1,
            SqlLogging::All => 2,
        }
    }

    pub(super) fn from_code(code: u8) -> SqlLogging {
        match code {
            0 => SqlLogging::Off,
            2 => SqlLogging::All,
            _ => SqlLogging::Slow,
        }
    }
}
