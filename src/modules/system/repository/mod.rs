//! Persistence layer for system installations and onboarding state.

mod system_repository;

pub(crate) use system_repository::complete_installation;
pub(crate) use system_repository::count_users;
pub(crate) use system_repository::create_installation;
pub(crate) use system_repository::get_installation;
