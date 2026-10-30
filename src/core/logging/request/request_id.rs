tokio::task_local! {
    /// The current request's id, readable from anywhere in the handler's
    /// task — used to forward `X-Request-Id` on outbound calls.
    pub(super) static REQUEST_ID: String;
}
