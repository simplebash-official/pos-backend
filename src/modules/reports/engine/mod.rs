// Reports, Analytics & Chart Engine.
// Coordinates high-performance in-memory caching, singleflight coalescing,
// parallel multi-pipeline execution, and automated cache invalidation.

pub mod cache;
pub mod charts;
pub mod feed;
pub mod indexes;
pub mod singleflight;

use std::sync::Arc;
use std::time::Instant;

use chrono::{Duration, Utc};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::{DENY_TENANT, Tenant};
use crate::domain::reports::{EngineFeedQuery, EngineFeedResponse, EngineMetadata};

use self::cache::{ReportsCache, build_scoped_cache_key, tenant_tag};
use self::feed::build_engine_feed;
use self::indexes::ensure_analytics_indexes;
use self::singleflight::SingleFlightGroup;

#[derive(Clone)]
pub struct AnalyticsEngine {
    db: Db,
    cache: Arc<ReportsCache>,
    singleflight: Arc<SingleFlightGroup<Value>>,
}

impl AnalyticsEngine {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            cache: Arc::new(ReportsCache::new()),
            singleflight: Arc::new(SingleFlightGroup::new()),
        }
    }

    /// Run background index verification on MongoDB collections.
    pub async fn init(&self) {
        if let Some(mongo_db) = self.db.as_mongo() {
            ensure_analytics_indexes(mongo_db.unscoped()).await;
        }
    }

    /// Tenant the calling request is confined to (`None` on single-shop
    /// deployments). Read at call time from the ambient request tenant, so it is
    /// part of every cache/singleflight key and of every invalidation.
    fn tenant_scope(&self) -> Option<String> {
        match self.db.as_mongo().map(|m| m.effective_tenant()) {
            Some(Tenant::Id(id)) => Some(id.to_string()),
            Some(Tenant::Deny) => Some(DENY_TENANT.to_string()),
            _ => None,
        }
    }

    /// Access database handle.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Fetches a section feed through the high-performance cache and singleflight engine.
    pub async fn get_feed(&self, query: EngineFeedQuery) -> AppResult<EngineFeedResponse> {
        let start_time = Instant::now();
        let bypass_cache = query.bypass_cache.unwrap_or(false);

        let section = query.section.as_deref().unwrap_or("overview");
        let preset = query.preset.as_deref().unwrap_or("this_month");
        let from = query.from.as_deref().unwrap_or("");
        let to = query.to.as_deref().unwrap_or("");
        let gran = query.granularity.as_deref().unwrap_or("");
        let cmp = query.compare_previous.unwrap_or(false);

        let is_active_period =
            preset == "today" || preset == "this_week" || preset == "this_month" || to.is_empty();
        let scope = self.tenant_scope();
        let cache_key = build_scoped_cache_key(
            scope.as_deref(),
            "feed",
            &format!("{section}:{preset}:{from}:{to}:{gran}:{cmp}"),
        );

        // 1. Check in-memory cache unless bypassed
        if !bypass_cache
            && let Some((val, cached_at, ttl_rem)) = self.cache.get(&cache_key)
            && let Ok(mut feed) = serde_json::from_value::<EngineFeedResponse>(val)
        {
            feed.meta = EngineMetadata {
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                cache_hit: true,
                cached_at,
                cache_status: format!("HIT (TTL: {ttl_rem}s remaining)"),
            };
            return Ok(feed);
        }

        // 2. Cache miss: execute via singleflight coalescer
        let db = self.db.clone();
        let query_clone = query.clone();
        let cache_key_clone = cache_key.clone();

        let computed_val = self
            .singleflight
            .work(&cache_key, move || async move {
                let initial_meta = EngineMetadata {
                    execution_time_ms: 0,
                    cache_hit: false,
                    cached_at: Utc::now(),
                    cache_status: "MISS".to_string(),
                };

                let feed = build_engine_feed(&db, &query_clone, initial_meta).await?;
                serde_json::to_value(feed).map_err(|e| AppError::internal(e.to_string()))
            })
            .await?;

        let mut feed: EngineFeedResponse = serde_json::from_value(computed_val.clone())
            .map_err(|e| AppError::internal(e.to_string()))?;

        // 3. Store into cache with appropriate TTL and tags
        let ttl = if is_active_period {
            Duration::seconds(60) // 1 minute for active open periods
        } else {
            Duration::hours(24) // 24 hours for closed historical periods
        };

        let mut tags = vec!["reports".to_string(), section.to_string()];
        if is_active_period {
            tags.push("active".to_string());
        }
        if let Some(t) = &scope {
            tags.push(tenant_tag(t));
        }

        self.cache.set(cache_key_clone, computed_val, ttl, tags);

        // Update timing metadata
        feed.meta.execution_time_ms = start_time.elapsed().as_millis() as u64;
        feed.meta.cache_status = "MISS (Calculated & Cached)".to_string();

        Ok(feed)
    }

    /// Generic cached-or-computed helper for individual legacy/standard reporting endpoints.
    pub async fn get_cached_or_compute<T, F, Fut>(
        &self,
        prefix: &str,
        params_key: &str,
        is_active: bool,
        tags: Vec<&str>,
        compute: F,
    ) -> AppResult<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync + 'static,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        let scope = self.tenant_scope();
        let cache_key = build_scoped_cache_key(scope.as_deref(), prefix, params_key);

        // 1. Check cache
        if let Some((val, _, _)) = self.cache.get(&cache_key)
            && let Ok(result) = serde_json::from_value::<T>(val)
        {
            return Ok(result);
        }

        // 2. Compute via SingleFlight
        let computed_val = self
            .singleflight
            .work(&cache_key, move || async move {
                let res = compute().await?;
                serde_json::to_value(res).map_err(|e| AppError::internal(e.to_string()))
            })
            .await?;

        let result: T = serde_json::from_value(computed_val.clone())
            .map_err(|e| AppError::internal(e.to_string()))?;

        // 3. Store
        let ttl = if is_active {
            Duration::seconds(60)
        } else {
            Duration::hours(24)
        };

        let mut all_tags: Vec<String> = tags.into_iter().map(String::from).collect();
        if is_active {
            all_tags.push("active".to_string());
        }
        if let Some(t) = &scope {
            all_tags.push(tenant_tag(t));
        }

        self.cache.set(cache_key, computed_val, ttl, all_tags);

        Ok(result)
    }

    /// Invalidate active open periods when transactions happen.
    pub fn invalidate_active(&self) -> usize {
        self.invalidate_tag("active")
    }

    /// Invalidate entries by tag.
    pub fn invalidate_tag(&self, tag: &str) -> usize {
        match self.tenant_scope() {
            None => self.cache.invalidate_tag(tag),
            Some(t) => self.cache.invalidate_matching(&[tag, &tenant_tag(&t)]),
        }
    }

    /// Clear entire reports cache.
    pub fn invalidate_all(&self) -> usize {
        match self.tenant_scope() {
            None => self.cache.invalidate_all(),
            Some(t) => self.cache.invalidate_matching(&[&tenant_tag(&t)]),
        }
    }
}
