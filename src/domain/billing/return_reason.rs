use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Reason why an item was returned by a customer — independent of its
/// physical `ItemCondition`; a customer can return a `Resalable` item for
/// `WarrantyClaim`, or a `Damaged` one for `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReturnReason {
    #[serde(alias = "defective")]
    Defective,
    #[serde(alias = "wrong_item", alias = "wrongItem")]
    WrongItem,
    #[serde(alias = "customer_changed_mind", alias = "customerChangedMind")]
    CustomerChangedMind,
    #[serde(alias = "warranty_claim", alias = "warrantyClaim")]
    WarrantyClaim,
    #[serde(alias = "other")]
    Other,
}
