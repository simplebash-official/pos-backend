//! Warranties: the profile a product carries, the warranty record each covered unit gets at
//! delivery, and its activity log. Pure types; persistence lives in `modules::warranties`.

mod claim;
mod profile;
mod warranty;

pub use claim::ClaimAction;
pub use claim::ClaimDecision;
pub use claim::ClaimListQuery;
pub use claim::ClaimListResponse;
pub use claim::ClaimListStatus;
pub use claim::ClaimStatus;
pub use claim::DecideClaimRequest;
pub use claim::InspectClaimRequest;
pub use claim::OpenClaimRequest;
pub use claim::ReplacementRequest;
pub use claim::ResolveClaimRequest;
pub use claim::SendToSupplierRequest;
pub use claim::SupplierClaim;
pub use claim::SupplierOutcome;
pub use claim::SupplierReturnRequest;
pub use claim::VoidChecks;
pub use claim::WarrantyClaim;
pub use claim::next_status;
pub use pos_calc_core::warranty::WarrantyCoverage;
pub use pos_calc_core::warranty::WarrantyUnit;
pub use profile::ProductWarranty;
pub use profile::WarrantyType;
pub use warranty::ExtendWarrantyRequest;
pub use warranty::VoidWarrantyRequest;
pub use warranty::Warranty;
pub use warranty::WarrantyDetail;
pub use warranty::WarrantyEvent;
pub use warranty::WarrantyEventKind;
pub use warranty::WarrantyGroup;
pub use warranty::WarrantyInvoiceLink;
pub use warranty::WarrantyListQuery;
pub use warranty::WarrantyListResponse;
pub use warranty::WarrantyLookupMatch;
pub use warranty::WarrantyLookupQuery;
pub use warranty::WarrantyLookupResponse;
pub use warranty::WarrantySourceType;
pub use warranty::WarrantyState;
pub use warranty::WarrantyStatus;
