/// Tenant value used by the fail-closed scope. Nothing legitimate ever has this
/// id, so reads match nothing; a write that slips through lands here, visibly
/// quarantined, instead of in a real tenant.
pub const DENY_TENANT: &str = "__deny__";
