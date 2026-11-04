//! Mongo and SQLite access for `warranties`, `warranty_events` and `warranty_claims` only.
//! Nothing here decides anything: the rules live in `service`.

mod claims_repository;
mod warranties_repository;

pub(crate) use claims_repository::ClaimFilter;
pub(crate) use claims_repository::find_claim;
pub(crate) use claims_repository::insert_claim;
pub(crate) use claims_repository::list_claims;
pub(crate) use claims_repository::replace_claim;
pub(crate) use warranties_repository::WarrantyFilter;
pub(crate) use warranties_repository::find_by_key;
pub(crate) use warranties_repository::insert_event;
pub(crate) use warranties_repository::insert_warranty;
pub(crate) use warranties_repository::list;
pub(crate) use warranties_repository::list_events;
pub(crate) use warranties_repository::replace_warranty;
