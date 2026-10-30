// Pure domain types for initial system installation, onboarding status, and database bootstrap.

mod setup_status_response;
mod setup_system_request;
mod setup_system_response;
mod system_installation;

pub use setup_status_response::SetupStatusResponse;
pub use setup_system_request::SetupSystemRequest;
pub use setup_system_response::SetupSystemResponse;
pub use system_installation::SystemInstallation;
