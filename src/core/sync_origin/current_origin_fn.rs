use super::current_origin::CURRENT_ORIGIN;

use std::sync::Arc;

/// The writer of the running request, if one was put in scope.
pub fn current_origin() -> Option<Arc<str>> {
    CURRENT_ORIGIN.try_with(Clone::clone).ok()
}
