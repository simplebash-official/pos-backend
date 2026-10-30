#![forbid(unsafe_code)]

/// Payload keys that describe the write itself (or are computed on read) and
/// therefore never count as a content difference.
pub const META_KEYS: &[&str] = &[
    "updatedAt",
    "version",
    "deviceId",
    "updatedByDevice",
    "isOverdue",
];
