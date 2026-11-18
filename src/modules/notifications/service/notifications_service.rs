use super::audience::{app_audience, check_service_audience, viewer_of};
use crate::clients::db::Db;
use crate::clients::uow::Unit;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::middleware::auth::CurrentUser;
use crate::domain::notifications::{
    CategoryUnread, CreateNotificationRequest, HistoryCursor, InternalNotificationRequest,
    MAX_MARK_KEYS, MAX_PAGE_LIMIT, MarkAllReadRequest, MarkReadResponse, NewNotification,
    Notification, NotificationCategory, NotificationFilter, NotificationListQuery,
    NotificationPage, NotificationSource, NotificationSummary, ReadState, SUMMARY_LATEST,
    notification_key_for, read_key_for,
};
use crate::modules::notifications::live;
use crate::modules::notifications::model::{NotificationDocument, NotificationReadDocument};
use crate::modules::notifications::repository::{self, Selection};
use chrono::{Duration, Utc};
use mongodb::bson::DateTime as BsonDateTime;

/// How long a notification is kept before the retention worker removes it.
pub const NOTIFICATION_RETENTION_DAYS: i64 = 180;

/// Raises a notification: the one way any part of the POS adds to the shop's history. Pass the
/// producer's own transaction so the alert is written (or not) together with the event that
/// caused it. A `dedupe_key` that was already used returns the stored notification unchanged.
pub(crate) async fn publish(
    unit: impl Into<Unit<'_>>,
    new: NewNotification,
) -> AppResult<Notification> {
    let mut unit = unit.into();
    let new = new.validated().map_err(AppError::validation)?;
    let key = match &new.dedupe_key {
        Some(dedupe) => notification_key_for(dedupe),
        None => generate_id(prefixes::NOTIFICATION),
    };
    if let Some(existing) = repository::find_by_key(&mut unit, &key).await? {
        return existing.into_notification(false);
    }
    let now = BsonDateTime::now();
    let (audience_type, audience_value) = NotificationDocument::stored_audience(&new.audience);
    let (link_path, link_label) = match new.link {
        Some(link) => (Some(link.path), Some(link.label)),
        None => (None, None),
    };
    let document = NotificationDocument {
        id: None,
        key,
        category: new.category,
        severity: new.severity,
        title: new.title,
        body: new.body,
        link_path,
        link_label,
        audience_type,
        audience_value,
        source: new.source,
        dedupe_key: new.dedupe_key,
        raised_by_user_key: new.raised_by_user_key,
        version: 1,
        created_at: now,
        updated_at: now,
        updated_by_device: None,
    };
    repository::insert(&mut unit, &document).await?;
    live::mark_changed();
    document.into_notification(false)
}

/// `POST /notifications`: an alert raised by the app for an audience the caller may address.
pub async fn create_from_app(
    db: &Db,
    caller: &CurrentUser,
    body: CreateNotificationRequest,
) -> AppResult<Notification> {
    let audience = app_audience(caller, body.audience)?;
    publish(
        db,
        NewNotification {
            category: body.category,
            severity: body.severity,
            title: body.title,
            body: body.body,
            link: body.link,
            audience,
            source: NotificationSource::Shop,
            dedupe_key: body.dedupe_key,
            raised_by_user_key: Some(caller.user_id.clone()),
        },
    )
    .await
}

/// The internal route: a notice sent by a trusted internal service to the shop in scope.
pub async fn create_from_service(
    db: &Db,
    body: InternalNotificationRequest,
) -> AppResult<Notification> {
    check_service_audience(&body.audience)?;
    publish(
        db,
        NewNotification {
            category: body.category,
            severity: body.severity,
            title: body.title,
            body: body.body,
            link: body.link,
            audience: body.audience,
            source: NotificationSource::Service,
            dedupe_key: Some(body.dedupe_key),
            raised_by_user_key: None,
        },
    )
    .await
}

/// One page of the caller's history, newest first.
pub async fn list(
    db: &Db,
    caller: &CurrentUser,
    query: NotificationListQuery,
) -> AppResult<NotificationPage> {
    if query.limit == 0 || query.limit > MAX_PAGE_LIMIT {
        return Err(AppError::validation(format!(
            "A page holds 1 to {MAX_PAGE_LIMIT} notifications"
        )));
    }
    let filter = NotificationFilter::parse(
        query.categories.as_deref(),
        query.severities.as_deref(),
        query.from,
        query.to,
        query.q.as_deref(),
    )
    .map_err(AppError::validation)?;
    let after = query
        .cursor
        .as_deref()
        .map(HistoryCursor::decode)
        .transpose()
        .map_err(AppError::validation)?;
    let limit = i64::from(query.limit);
    // One extra row tells whether another page follows.
    let mut rows = repository::list_visible(
        db,
        viewer_of(caller),
        Selection {
            filter: &filter,
            state: query.state,
            after: after.as_ref(),
        },
        limit + 1,
    )
    .await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(query.limit as usize);
    let next_cursor = match (has_more, rows.last()) {
        (true, Some((last, _))) => Some(
            HistoryCursor {
                created_at: last.created_at.to_chrono(),
                key: last.key.clone(),
            }
            .encode(),
        ),
        _ => None,
    };
    let items = rows
        .into_iter()
        .map(|(document, read)| document.into_notification(read))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(NotificationPage { items, next_cursor })
}

/// What the header bell shows for the caller.
pub async fn summary(db: &Db, caller: &CurrentUser) -> AppResult<NotificationSummary> {
    let viewer = viewer_of(caller);
    let counts = repository::unread_by_category(db, viewer).await?;
    let unread_by_category: Vec<CategoryUnread> = NotificationCategory::ALL
        .into_iter()
        .filter_map(|category| {
            counts
                .iter()
                .find(|(c, _)| *c == category)
                .map(|(_, unread)| CategoryUnread {
                    category,
                    unread: *unread,
                })
        })
        .collect();
    let everything = NotificationFilter {
        categories: Vec::new(),
        severities: Vec::new(),
        from: None,
        to: None,
        search: None,
    };
    let latest = repository::list_visible(
        db,
        viewer,
        Selection {
            filter: &everything,
            state: ReadState::All,
            after: None,
        },
        SUMMARY_LATEST,
    )
    .await?
    .into_iter()
    .map(|(document, read)| document.into_notification(read))
    .collect::<AppResult<Vec<_>>>()?;
    Ok(NotificationSummary {
        unread_total: unread_by_category.iter().map(|c| c.unread).sum(),
        unread_by_category,
        latest,
    })
}

/// Marks the given notifications read for the caller. Keys the caller cannot see are ignored.
pub async fn mark_read(
    db: &Db,
    caller: &CurrentUser,
    keys: Vec<String>,
) -> AppResult<MarkReadResponse> {
    if keys.is_empty() || keys.len() > MAX_MARK_KEYS {
        return Err(AppError::validation(format!(
            "Mark 1 to {MAX_MARK_KEYS} notifications at a time"
        )));
    }
    let visible = repository::visible_among(db, viewer_of(caller), &keys).await?;
    write_reads(db, &caller.user_id, &visible).await
}

/// Marks every unread notification matching the filter read for the caller.
pub async fn mark_all_read(
    db: &Db,
    caller: &CurrentUser,
    body: MarkAllReadRequest,
) -> AppResult<MarkReadResponse> {
    let filter = NotificationFilter::parse(
        body.categories.as_deref(),
        body.severities.as_deref(),
        body.from,
        body.to,
        body.q.as_deref(),
    )
    .map_err(AppError::validation)?;
    let keys = repository::visible_keys(
        db,
        viewer_of(caller),
        Selection {
            filter: &filter,
            state: ReadState::Unread,
            after: None,
        },
    )
    .await?;
    write_reads(db, &caller.user_id, &keys).await
}

async fn write_reads(db: &Db, user_key: &str, keys: &[String]) -> AppResult<MarkReadResponse> {
    let now = BsonDateTime::now();
    let documents: Vec<NotificationReadDocument> = keys
        .iter()
        .map(|notification_key| NotificationReadDocument {
            id: None,
            key: read_key_for(user_key, notification_key),
            notification_key: notification_key.clone(),
            user_key: user_key.to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        })
        .collect();
    let marked = repository::insert_reads_if_absent(db, &documents).await?;
    if marked > 0 {
        live::mark_changed();
    }
    Ok(MarkReadResponse { marked })
}

/// Removes notifications older than the retention window, in every shop. Each replica runs it
/// itself, so nothing is sent through sync.
pub async fn purge_expired(db: &Db) -> AppResult<u64> {
    let cutoff = Utc::now() - Duration::days(NOTIFICATION_RETENTION_DAYS);
    repository::purge_older_than(db, BsonDateTime::from_chrono(cutoff)).await
}
