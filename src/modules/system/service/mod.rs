// Business logic for initial system setup, onboarding, and conditional database seeding.

mod get_installation_info;
mod get_or_create_installation;
mod get_setup_status;
mod get_tenant_setup_status;
mod perform_setup;
mod perform_tenant_setup;
mod seed_sample_data;

pub(crate) use get_installation_info::get_installation_info;
pub(crate) use get_setup_status::get_setup_status;
pub(crate) use get_tenant_setup_status::get_tenant_setup_status;
pub(crate) use perform_setup::perform_setup;
pub(crate) use perform_tenant_setup::perform_tenant_setup;
