use std::collections::HashMap;

/// A shop's saved option values: design family -> option key -> value.
pub(crate) type OptionValues = HashMap<String, HashMap<String, serde_json::Value>>;
