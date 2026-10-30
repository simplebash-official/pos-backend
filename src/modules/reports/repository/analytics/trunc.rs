use super::super::super::service::dates::REPORT_TZ_MONGO;

use mongodb::bson::{Document, doc};

pub(super) fn trunc(unit: &str) -> Document {
    doc! {
        "$dateTrunc": {
            "date": "$created_at",
            "unit": unit,
            "binSize": 1,
            "timezone": REPORT_TZ_MONGO,
            "startOfWeek": "monday",
        }
    }
}
