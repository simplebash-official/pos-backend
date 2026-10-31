//! The one error the calculations return.

/// Why a calculation refused its input.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum CalcError {
    #[error("{0}")]
    Validation(String),
}

/// The result of a calculation.
pub type CalcResult<T> = Result<T, CalcError>;

impl CalcError {
    /// A validation error carrying a message fit to show the user.
    #[must_use]
    pub fn validation(msg: impl Into<String>) -> Self {
        CalcError::Validation(msg.into())
    }
}
