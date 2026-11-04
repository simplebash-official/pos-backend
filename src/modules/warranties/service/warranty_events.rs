use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;
use crate::domain::users::ApprovedBy;
use crate::domain::warranties::WarrantyEventKind;
use crate::modules::warranties::model::WarrantyEventDocument;
use crate::modules::warranties::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Who did something to a warranty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WarrantyActor {
    pub user_id: String,
    pub name: String,
}

/// One activity-log line to append.
#[derive(Debug)]
pub(crate) struct NewEvent<'a> {
    pub warranty_key: &'a str,
    pub claim_key: Option<&'a str>,
    pub kind: WarrantyEventKind,
    pub from_status: Option<&'a str>,
    pub to_status: Option<&'a str>,
    pub note: Option<String>,
    pub actor: &'a WarrantyActor,
    pub approved_by: Option<&'a ApprovedBy>,
}

/// Appends one line to a warranty's activity log.
pub(crate) async fn record(db: &Db, event: NewEvent<'_>) -> AppResult<()> {
    let now = BsonDateTime::now();
    repository::insert_event(
        db,
        WarrantyEventDocument {
            id: None,
            key: generate_id(prefixes::WARRANTY_EVENT),
            warranty_key: event.warranty_key.to_string(),
            claim_key: event.claim_key.map(str::to_string),
            kind: event.kind,
            from_status: event.from_status.map(str::to_string),
            to_status: event.to_status.map(str::to_string),
            note: event.note,
            actor_user_id: event.actor.user_id.clone(),
            actor_name: event.actor.name.clone(),
            approved_by_user_id: event.approved_by.map(|a| a.user_id.clone()),
            approved_by_name: event.approved_by.map(|a| a.name.clone()),
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        },
    )
    .await
}
