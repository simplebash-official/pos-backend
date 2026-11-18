//! Realtime announcements of notification changes to the shop's open screens. Announces only
//! that something changed, never what (the same rule as sync's live channel).

mod notifications_live;

pub use notifications_live::announce_after_write;
pub(crate) use notifications_live::events;
pub(crate) use notifications_live::mark_changed;
