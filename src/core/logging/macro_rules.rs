/// Record a business event (sale completed, stock adjusted, user created…)
/// as `category=domain`. Call it from the service layer, after the change
/// succeeded, with the identifying keys as fields:
///
/// ```ignore
/// log_event!("sale.created", invoice_key = %invoice.key, total_cents = invoice.total, "Sale completed");
/// ```
///
/// The request's `request_id` / `user_id` are attached automatically from
/// the request span.
#[macro_export]
macro_rules! log_event {
    ($event:literal, $($rest:tt)+) => {
        ::tracing::info!(target: "domain", category = "domain", event = $event, $($rest)+)
    };
    ($event:literal) => {
        ::tracing::info!(target: "domain", category = "domain", event = $event, $event)
    };
}
