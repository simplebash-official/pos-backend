use mongodb::bson::DateTime as BsonDateTime;

pub(super) fn millis(ms: i64) -> BsonDateTime {
    BsonDateTime::from_millis(ms)
}
