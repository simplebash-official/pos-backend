//! Pure business types for settings and shop profile persistence.

mod domain_settings;

pub use domain_settings::CurrentDocumentDesign;
pub use domain_settings::DesignOption;
pub use domain_settings::DesignOptionChoice;
pub use domain_settings::DesignOptionType;
pub use domain_settings::DocumentDesignChoice;
pub use domain_settings::DocumentDesignFamily;
pub use domain_settings::DocumentDesignStatus;
pub use domain_settings::DocumentKindSettings;
pub use domain_settings::DocumentTemplatesResponse;
pub use domain_settings::ShopProfileResponse;
pub use domain_settings::StoredDocumentDesign;
pub use domain_settings::UpdateDocumentTemplatesRequest;
pub use domain_settings::UpdateShopProfileRequest;
