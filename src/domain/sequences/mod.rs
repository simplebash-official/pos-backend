//! Request and response types for reserving document-number sequences.

mod domain_sequences;

pub use domain_sequences::ReserveSequenceRequest;
pub use domain_sequences::SequenceReservationResponse;
