use std::collections::HashMap;

/// A shop's stored choices: document kind slug -> design family.
pub(crate) type Selections = HashMap<String, String>;
