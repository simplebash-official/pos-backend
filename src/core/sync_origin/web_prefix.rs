/// Prefix for a browser session's `X-Device-Id`. Keeps a browser from ever
/// sharing an id with a registered device (which would hide that device's
/// own data from it).
pub const WEB_PREFIX: &str = "web:";
