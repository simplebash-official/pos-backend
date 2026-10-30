use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A contact person within a supplier business. Each contact person has a name
/// and between 1 and 2 telephone numbers (maximum 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContactPerson {
    /// Full name of the contact person.
    pub name: String,
    /// Up to 2 telephone numbers for this contact person.
    pub phone_numbers: Vec<String>,
}
