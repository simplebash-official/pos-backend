/// Generates a 24-character hexadecimal identifier matching MongoDB ObjectId format.
#[must_use]
pub fn generate_id_hex() -> String {
    mongodb::bson::oid::ObjectId::new().to_hex()
}
