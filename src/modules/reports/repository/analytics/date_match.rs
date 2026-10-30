use chrono::{DateTime, Utc};

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

pub(super) fn date_match(start: DateTime<Utc>, end: DateTime<Utc>) -> Document {
    doc! {
        "status": { "$ne": "voided" },
        "created_at": {
            "$gte": BsonDateTime::from_chrono(start),
            "$lt": BsonDateTime::from_chrono(end),
        }
    }
}
