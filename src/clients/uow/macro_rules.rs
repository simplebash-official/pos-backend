/// Runs a Mongo action inside the transaction's session when there is one.
/// `$action` is the driver builder (`find_one(..)`, `update_one(..)`, ...) and
/// `$session` the `Option<&mut ClientSession>` from `Backend::Mongo`.
#[macro_export]
macro_rules! in_session {
    ($action:expr, $session:expr) => {
        match $session {
            Some(session) => $action.session(session).await,
            None => $action.await,
        }
    };
}
