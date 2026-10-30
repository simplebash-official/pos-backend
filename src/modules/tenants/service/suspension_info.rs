use super::retained::Retained;

/// What a suspended shop's staff are shown: when and why the account was suspended, and proof that
/// the shop's data is still stored.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuspensionInfo {
    pub shop_name: String,
    /// RFC 3339.
    pub suspended_at: String,
    pub reason: String,
    pub retained: Retained,
}
