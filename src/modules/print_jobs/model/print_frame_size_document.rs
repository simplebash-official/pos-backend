use crate::domain::print_jobs::PrintFrameSize;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A stored frame size (`print_frame_sizes`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintFrameSizeDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub size: String,
    pub width_hundredths_inch: i64,
    pub active: bool,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl PrintFrameSizeDocument {
    /// The frame size as clients see it.
    #[must_use]
    pub fn into_print_frame_size(self) -> PrintFrameSize {
        PrintFrameSize {
            id: self
                .id
                .expect("a stored frame size always has an id")
                .to_hex(),
            key: self.key,
            size: self.size,
            width_hundredths_inch: self.width_hundredths_inch,
            active: self.active,
            version: self.version,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            updated_by_device: self.updated_by_device,
        }
    }
}
