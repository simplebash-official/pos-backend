use super::resolved_template::ResolvedTemplate;

/// The `generated_documents` cache key for a print. The built-in default keeps
/// the kind's historical key, so every PDF cached before designs were
/// selectable stays valid; any other template gets its own key (per design
/// *version*, because the name is immutable), so a design is never served
/// from another design's cache. A design with options also carries a digest of
/// the option values, so changing a setting renders afresh instead of serving
/// the PDF cached under the old values.
pub(crate) fn cache_key(legacy_key: &str, resolved: &ResolvedTemplate) -> String {
    if resolved.is_default {
        legacy_key.to_string()
    } else if resolved.option_values.is_empty() {
        format!("{legacy_key}--{}", resolved.info.name)
    } else {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(
            serde_json::Value::Object(resolved.option_values.clone())
                .to_string()
                .as_bytes(),
        );
        let short: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
        format!("{legacy_key}--{}--{short}", resolved.info.name)
    }
}
