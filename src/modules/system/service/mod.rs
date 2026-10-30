//! Business logic for initial system setup, onboarding, and conditional database seeding.

mod system_service;

pub(crate) use system_service::get_installation_info;
pub(crate) use system_service::get_setup_status;
pub(crate) use system_service::get_tenant_setup_status;
pub(crate) use system_service::perform_setup;
pub(crate) use system_service::perform_tenant_setup;
