// Service module exports for register sessions, cash movements, and Z-report generation.

pub mod movement;
pub mod session;
pub mod z_report;

pub use movement::*;
pub use session::*;
pub use z_report::*;

use mongodb::bson::Document;
use crate::{
    core::error::AppResult,
    domain::sessions::{CashMovement, RegisterSession},
    modules::sessions::model::{CashMovementDocument, RegisterSessionDocument},
};

pub(crate) fn hydrate_session_sync_documents(documents: Vec<Document>) -> AppResult<Vec<RegisterSession>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<RegisterSessionDocument>(document)?.into_register_session())
        })
        .collect()
}

pub(crate) fn hydrate_movement_sync_documents(documents: Vec<Document>) -> AppResult<Vec<CashMovement>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(bson::deserialize_from_document::<CashMovementDocument>(document)?.into_cash_movement())
        })
        .collect()
}

