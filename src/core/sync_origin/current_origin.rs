use std::sync::Arc;

tokio::task_local! {
    pub(super) static CURRENT_ORIGIN: Arc<str>;
}
