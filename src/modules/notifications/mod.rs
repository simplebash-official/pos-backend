//! The shop's central notification engine. Any module raises an alert through
//! `service::publish` (inside its own transaction); the app raises its own through
//! `POST /notifications`; trusted internal services through the internal route. Each user sees
//! what is addressed to them, with their own read state, live via `GET /notifications/events`.
//! Notifications and read marks sync to every terminal and are kept for
//! `NOTIFICATION_RETENTION_DAYS`. Brief pop-up toasts are the frontend's and never stored here.

pub mod live;
pub mod model;
mod repository;
pub mod routes;
pub mod service;
