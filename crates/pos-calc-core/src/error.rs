#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum CalcError {
    #[error("{0}")]
    Validation(String),
}

pub type CalcResult<T> = Result<T, CalcError>;

impl CalcError {
    pub fn validation(msg: impl Into<String>) -> Self {
        CalcError::Validation(msg.into())
    }
}
