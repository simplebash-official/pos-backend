/// Payload fields that travel between a device and the cloud but must never be
/// written anywhere else (conflict records, logs): the `users` password hash.
/// The hash is needed on the wire so a cashier created on one device can log in
/// on every other device and on the cloud.
#[must_use]
pub fn secret_fields(resource: &str) -> &'static [&'static str] {
    match resource {
        "users" => &["passwordHash", "pinHash"],
        _ => &[],
    }
}
