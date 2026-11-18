use super::notifications_repository::{READS, notification_from_row, notifications};
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::{AppError, AppResult};
use crate::domain::notifications::{
    HistoryCursor, NotificationCategory, NotificationFilter, ReadState,
};
use crate::modules::notifications::model::NotificationDocument;
use futures_util::TryStreamExt;
use mongodb::bson::{self, Bson, DateTime as BsonDateTime, Document, doc};
use sqlx::{QueryBuilder, Row, Sqlite};

/// The person reading: what decides which notifications they see and which they have read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Viewer<'a> {
    pub user_key: &'a str,
    pub permissions: &'a [String],
}

/// What to select, besides the viewer's audience.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Selection<'a> {
    pub filter: &'a NotificationFilter,
    pub state: ReadState,
    /// Only rows after this position (newest first).
    pub after: Option<&'a HistoryCursor>,
}

/// Up to `limit` visible notifications, newest first, each with whether the viewer read it.
pub(crate) async fn list_visible(
    db: &Db,
    viewer: Viewer<'_>,
    selection: Selection<'_>,
    limit: i64,
) -> AppResult<Vec<(NotificationDocument, bool)>> {
    match db {
        Db::Mongo(db) => {
            let mut pipeline = mongo_selection(viewer, selection);
            pipeline.push(doc! { "$sort": { "created_at": -1, "key": -1 } });
            pipeline.push(doc! { "$limit": limit });
            let mut cursor = notifications(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(mut row) = cursor.try_next().await? {
                let read = row.get_bool("read").map_err(|err| {
                    AppError::internal(format!("notification read state is missing: {err}"))
                })?;
                row.remove("read");
                row.remove("reads");
                out.push((bson::deserialize_from_document(row)?, read));
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut qb = QueryBuilder::<Sqlite>::new("SELECT n.*, ");
            push_is_read(&mut qb, viewer);
            qb.push(" AS is_read FROM notifications n WHERE ");
            push_selection(&mut qb, viewer, selection);
            qb.push(" ORDER BY julianday(n.created_at) DESC, n.key DESC LIMIT ");
            qb.push_bind(limit);
            let rows = qb.build().fetch_all(pool).await?;
            rows.iter()
                .map(|row| {
                    Ok((
                        notification_from_row(row)?,
                        row.get::<i64, _>("is_read") != 0,
                    ))
                })
                .collect()
        }
    }
}

/// Keys of every visible notification matching the selection (no limit).
pub(crate) async fn visible_keys(
    db: &Db,
    viewer: Viewer<'_>,
    selection: Selection<'_>,
) -> AppResult<Vec<String>> {
    match db {
        Db::Mongo(db) => {
            let mut pipeline = mongo_selection(viewer, selection);
            pipeline.push(doc! { "$project": { "_id": 0, "key": 1 } });
            let mut cursor = notifications(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                out.push(
                    row.get_str("key")
                        .map_err(|err| AppError::internal(format!("notification key: {err}")))?
                        .to_string(),
                );
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut qb = QueryBuilder::<Sqlite>::new("SELECT n.key FROM notifications n WHERE ");
            push_selection(&mut qb, viewer, selection);
            Ok(qb.build_query_scalar().fetch_all(pool).await?)
        }
    }
}

/// Of `keys`, the ones the viewer may see.
pub(crate) async fn visible_among(
    db: &Db,
    viewer: Viewer<'_>,
    keys: &[String],
) -> AppResult<Vec<String>> {
    match db {
        Db::Mongo(db) => {
            let filter = doc! { "$and": [audience_match(viewer), { "key": { "$in": keys } }] };
            Ok(notifications(db)
                .distinct("key", filter)
                .await?
                .into_iter()
                .filter_map(|b| b.as_str().map(str::to_string))
                .collect())
        }
        Db::Sqlite(pool) => {
            let mut qb = QueryBuilder::<Sqlite>::new("SELECT n.key FROM notifications n WHERE ");
            push_audience(&mut qb, viewer);
            qb.push(" AND n.key IN (");
            let mut list = qb.separated(", ");
            for key in keys {
                list.push_bind(key);
            }
            qb.push(")");
            Ok(qb.build_query_scalar().fetch_all(pool).await?)
        }
    }
}

/// Unread visible notifications per category (categories without any are left out).
pub(crate) async fn unread_by_category(
    db: &Db,
    viewer: Viewer<'_>,
) -> AppResult<Vec<(NotificationCategory, u64)>> {
    let everything = NotificationFilter {
        categories: Vec::new(),
        severities: Vec::new(),
        from: None,
        to: None,
        search: None,
    };
    let selection = Selection {
        filter: &everything,
        state: ReadState::Unread,
        after: None,
    };
    let unknown = |value: &str| {
        AppError::internal(format!(
            "unknown notification category '{value}' in database"
        ))
    };
    match db {
        Db::Mongo(db) => {
            let mut pipeline = mongo_selection(viewer, selection);
            pipeline.push(doc! { "$group": { "_id": "$category", "count": { "$sum": 1 } } });
            let mut cursor = notifications(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let name = row
                    .get_str("_id")
                    .map_err(|err| AppError::internal(format!("notification category: {err}")))?;
                let count = match row.get("count") {
                    Some(Bson::Int32(n)) => u64::try_from(*n),
                    Some(Bson::Int64(n)) => u64::try_from(*n),
                    other => {
                        return Err(AppError::internal(format!(
                            "notification count is not a number: {other:?}"
                        )));
                    }
                }
                .map_err(|err| AppError::internal(format!("notification count: {err}")))?;
                out.push((
                    NotificationCategory::parse(name).ok_or_else(|| unknown(name))?,
                    count,
                ));
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT n.category AS category, COUNT(*) AS unread FROM notifications n WHERE ",
            );
            push_selection(&mut qb, viewer, selection);
            qb.push(" GROUP BY n.category");
            let rows = qb.build().fetch_all(pool).await?;
            rows.iter()
                .map(|row| {
                    let name: String = row.get("category");
                    let count = u64::try_from(row.get::<i64, _>("unread"))
                        .map_err(|err| AppError::internal(format!("notification count: {err}")))?;
                    Ok((
                        NotificationCategory::parse(&name).ok_or_else(|| unknown(&name))?,
                        count,
                    ))
                })
                .collect()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// MongoDB
// ---------------------------------------------------------------------------------------------

fn audience_match(viewer: Viewer<'_>) -> Document {
    doc! { "$or": [
        { "audience_type": "everyone" },
        { "audience_type": "user", "audience_value": viewer.user_key },
        { "audience_type": "permission", "audience_value": { "$in": viewer.permissions } },
    ] }
}

/// `$match` on audience and filter, then the viewer's read state as `read`.
fn mongo_selection(viewer: Viewer<'_>, selection: Selection<'_>) -> Vec<Document> {
    let filter = selection.filter;
    let mut conditions = vec![audience_match(viewer)];
    if !filter.categories.is_empty() {
        let names: Vec<&str> = filter.categories.iter().map(|c| c.as_str()).collect();
        conditions.push(doc! { "category": { "$in": names } });
    }
    if !filter.severities.is_empty() {
        let names: Vec<&str> = filter.severities.iter().map(|s| s.as_str()).collect();
        conditions.push(doc! { "severity": { "$in": names } });
    }
    if let Some(from) = filter.from {
        conditions.push(doc! { "created_at": { "$gte": BsonDateTime::from_chrono(from) } });
    }
    if let Some(to) = filter.to {
        conditions.push(doc! { "created_at": { "$lt": BsonDateTime::from_chrono(to) } });
    }
    if let Some(search) = &filter.search {
        let pattern = regex::escape(search);
        conditions.push(doc! { "$or": [
            { "title": { "$regex": &pattern, "$options": "i" } },
            { "body": { "$regex": &pattern, "$options": "i" } },
        ] });
    }
    if let Some(after) = selection.after {
        let at = BsonDateTime::from_chrono(after.created_at);
        conditions.push(doc! { "$or": [
            { "created_at": { "$lt": at } },
            { "created_at": at, "key": { "$lt": &after.key } },
        ] });
    }
    let mut pipeline = vec![
        doc! { "$match": { "$and": conditions } },
        doc! { "$lookup": {
            "from": READS,
            "localField": "key",
            "foreignField": "notification_key",
            "as": "reads",
        } },
        doc! { "$addFields": { "read": { "$in": [viewer.user_key, "$reads.user_key"] } } },
    ];
    match selection.state {
        ReadState::All => {}
        ReadState::Unread => pipeline.push(doc! { "$match": { "read": false } }),
        ReadState::Read => pipeline.push(doc! { "$match": { "read": true } }),
    }
    pipeline
}

// ---------------------------------------------------------------------------------------------
// SQLite
// ---------------------------------------------------------------------------------------------

fn push_is_read<'a>(qb: &mut QueryBuilder<'a, Sqlite>, viewer: Viewer<'a>) {
    qb.push(
        "EXISTS (SELECT 1 FROM notification_reads r WHERE r.notification_key = n.key \
         AND r.user_key = ",
    );
    qb.push_bind(viewer.user_key);
    qb.push(")");
}

fn push_audience<'a>(qb: &mut QueryBuilder<'a, Sqlite>, viewer: Viewer<'a>) {
    qb.push("(n.audience_type = 'everyone' OR (n.audience_type = 'user' AND n.audience_value = ");
    qb.push_bind(viewer.user_key);
    qb.push(")");
    if !viewer.permissions.is_empty() {
        qb.push(" OR (n.audience_type = 'permission' AND n.audience_value IN (");
        let mut list = qb.separated(", ");
        for permission in viewer.permissions {
            list.push_bind(permission.as_str());
        }
        qb.push("))");
    }
    qb.push(")");
}

fn push_selection<'a>(
    qb: &mut QueryBuilder<'a, Sqlite>,
    viewer: Viewer<'a>,
    selection: Selection<'a>,
) {
    let filter = selection.filter;
    push_audience(qb, viewer);
    if !filter.categories.is_empty() {
        qb.push(" AND n.category IN (");
        let mut list = qb.separated(", ");
        for category in &filter.categories {
            list.push_bind(category.as_str());
        }
        qb.push(")");
    }
    if !filter.severities.is_empty() {
        qb.push(" AND n.severity IN (");
        let mut list = qb.separated(", ");
        for severity in &filter.severities {
            list.push_bind(severity.as_str());
        }
        qb.push(")");
    }
    if let Some(from) = filter.from {
        qb.push(" AND julianday(n.created_at) >= julianday(");
        qb.push_bind(bson_to_iso(&BsonDateTime::from_chrono(from)));
        qb.push(")");
    }
    if let Some(to) = filter.to {
        qb.push(" AND julianday(n.created_at) < julianday(");
        qb.push_bind(bson_to_iso(&BsonDateTime::from_chrono(to)));
        qb.push(")");
    }
    if let Some(search) = &filter.search {
        let needle = search.to_lowercase();
        qb.push(" AND (instr(lower(n.title), ");
        qb.push_bind(needle.clone());
        qb.push(") > 0 OR instr(lower(n.body), ");
        qb.push_bind(needle);
        qb.push(") > 0)");
    }
    if let Some(after) = selection.after {
        let at = bson_to_iso(&BsonDateTime::from_chrono(after.created_at));
        qb.push(" AND (julianday(n.created_at) < julianday(");
        qb.push_bind(at.clone());
        qb.push(") OR (julianday(n.created_at) = julianday(");
        qb.push_bind(at);
        qb.push(") AND n.key < ");
        qb.push_bind(after.key.as_str());
        qb.push("))");
    }
    match selection.state {
        ReadState::All => {}
        ReadState::Unread => {
            qb.push(" AND NOT ");
            push_is_read(qb, viewer);
        }
        ReadState::Read => {
            qb.push(" AND ");
            push_is_read(qb, viewer);
        }
    }
}
