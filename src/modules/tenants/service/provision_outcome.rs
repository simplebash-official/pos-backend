/// What a provisioning call actually changed; both `false` on a pure replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ProvisionOutcome {
    pub tenant_created: bool,
    pub admin_created: bool,
}
