use super::tenant::Tenant;

tokio::task_local! {
    pub(super) static CURRENT_TENANT: Tenant;
}
