use super::change_ack::ChangeAck;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PushResponse {
    pub server_time: DateTime<Utc>,
    pub acks: Vec<ChangeAck>,
    pub server_seq: i64,
}
