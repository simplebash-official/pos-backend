use super::synced_collections::SYNCED_COLLECTIONS;

#[must_use]
pub fn is_synced_collection(name: &str) -> bool {
    SYNCED_COLLECTIONS.contains(&name)
}
