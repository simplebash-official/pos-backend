use super::attachment_owner::AttachmentOwner;

use serde::Deserialize;
use utoipa::IntoParams;

/// Query for `GET /attachments`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AttachmentListQuery {
    pub owner_kind: AttachmentOwner,
    pub owner_key: String,
}
