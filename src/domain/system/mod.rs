//! Pure domain types for initial system installation, onboarding status, and database bootstrap.

mod domain_system;

pub use domain_system::SetupStatusResponse;
pub use domain_system::SetupSystemRequest;
pub use domain_system::SetupSystemResponse;
pub use domain_system::SystemInstallation;
