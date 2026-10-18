// HTTP client for the sibling document-server (Typst PDF rendering,
// `../document-server`). This is the *only* caller of that service anywhere
// in the system — the frontend never talks to it directly (see
// `modules::documents::service`, which is the only module allowed to call
// this client). Built once at startup and shared read-only via `AppState`,
// the same "connect/build once, hand out a shared handle" shape as
// `clients::mongo::connect`.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;

use crate::{
    core::{
        error::{AppError, AppResult},
        response::ApiResponse,
    },
    domain::settings::DesignOption,
};

/// How long a fetched template list is trusted. Designs are published on the
/// document-server (`POST /api/packages`) without a restart, so the list must
/// refresh by itself; a minute keeps the per-print cost at zero while a new
/// design shows up almost immediately.
const TEMPLATE_CACHE_TTL: Duration = Duration::from_secs(60);

/// One published template as the document-server lists it for the calling
/// tenant: the **latest published version** of a design family that is either
/// bundled (global) or granted to the tenant.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// The document-server-minted unique key (`tpl_...`) — persisted onto
    /// `generated_documents` rows so the POS side durably holds the ID the
    /// document server assigned. Differs per document-server database.
    pub key: String,
    /// The opaque, immutable template id (`doc_temp_<nanoid>`); one per
    /// version. Identical on every deployment.
    pub name: String,
    /// The design this version belongs to; a shop selects a family and always
    /// prints with its latest version.
    pub family: String,
    pub version: i64,
    /// What the template is for (`a4-invoice`, `thermal-receipt`, …).
    pub kind: String,
    /// Human label (the schema `title`) shown in the dropdowns.
    #[serde(rename = "description")]
    pub title: String,
    /// `document` or `label`.
    pub r#type: String,
    /// `global` (bundled) or `private` (a published package).
    pub scope: String,
    /// The built-in default of its kind (`x-default`).
    pub is_default: bool,
    /// Controls the template declares (`x-options`); their values travel in
    /// the payload's `options` object.
    pub options: Vec<DesignOption>,
    /// JSON Schema for the template's render payload (`dataSchema`) — the one
    /// contract this backend codes its payload builders against.
    pub data_schema: serde_json::Value,
    /// The template's worked sample payload (`data`), used for previews.
    #[serde(rename = "data")]
    pub sample: serde_json::Value,
    pub sha256: String,
    pub min_app_version: Option<String>,
}

struct TemplateCache {
    loaded_at: Instant,
    templates: Arc<Vec<TemplateInfo>>,
}

pub struct DocumentServerClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    cache_ttl: Duration,
    // The templates from `GET /api/templates`, one list per tenant (the
    // document-server filters it by `X-Tenant-Key`; the key here is
    // `current_tenant_id()`, empty outside a tenant), each refreshed after
    // `cache_ttl`. If a refresh fails while a previous list exists, that list
    // keeps being served (with a warning): a document-server hiccup must not
    // stop the shop from printing. The schema rides along in the same entry
    // because `render_template()` validates every payload against it *before*
    // sending.
    template_cache: RwLock<HashMap<String, TemplateCache>>,
}

#[derive(serde::Deserialize)]
struct TemplatesResponse {
    templates: Vec<TemplateInfo>,
}

impl DocumentServerClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            api_key,
            cache_ttl: TEMPLATE_CACHE_TTL,
            template_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Overrides how long the template list is trusted (tests use zero to
    /// force a refresh on every call).
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Polls `GET {base_url}/api/health` until it responds successfully or
    /// `max_attempts` is exhausted, sleeping `250ms * attempt` between tries
    /// (mirrors `clients::mongo::connect`'s retry shape). Called once at
    /// startup, before this client is handed to `AppState`, so a
    /// document-server that's still booting doesn't look like a silently
    /// broken one — the backend only starts serving once it can actually
    /// reach it. Returns `Result<(), String>` rather than `AppResult` since
    /// this is only ever called from `main.rs` startup logging, never from a
    /// request handler, and `AppError` has no `Display` impl to log with.
    pub async fn wait_until_ready(&self, max_attempts: u32) -> Result<(), String> {
        let url = format!("{}/api/health", self.base_url);
        let mut last_err = None;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(250 * attempt as u64)).await;
            }

            match self.http.get(&url).send().await {
                Ok(response) if response.status().is_success() => return Ok(()),
                Ok(response) => last_err = Some(format!("status {}", response.status())),
                Err(err) => last_err = Some(err.to_string()),
            }
        }

        Err(format!(
            "document-server did not become ready at {} after {} attempts: {}",
            self.base_url,
            max_attempts,
            last_err.unwrap_or_else(|| "unknown error".to_string())
        ))
    }

    /// Every template the calling tenant may use, from the cache while it is
    /// fresh, else from `GET /api/templates`.
    pub async fn list_templates(&self) -> AppResult<Arc<Vec<TemplateInfo>>> {
        let tenant = crate::core::tenancy::current_tenant_id().unwrap_or_default();
        if let Some(cache) = self.template_cache.read().await.get(&tenant)
            && cache.loaded_at.elapsed() < self.cache_ttl
        {
            return Ok(cache.templates.clone());
        }

        match self.fetch_templates().await {
            Ok(templates) => {
                let templates = Arc::new(templates);
                self.template_cache.write().await.insert(
                    tenant,
                    TemplateCache {
                        loaded_at: Instant::now(),
                        templates: templates.clone(),
                    },
                );
                Ok(templates)
            }
            Err(err) => {
                if let Some(cache) = self.template_cache.read().await.get(&tenant) {
                    tracing::warn!(
                        category = "http",
                        event = "outbound",
                        service = "document-server",
                        "could not refresh the template list, serving the previous one"
                    );
                    return Ok(cache.templates.clone());
                }
                Err(err)
            }
        }
    }

    async fn fetch_templates(&self) -> AppResult<Vec<TemplateInfo>> {
        let url = format!("{}/api/templates", self.base_url);
        let response = self
            .http
            .get(&url)
            .headers(forwarded_headers())
            .header("X-Internal-Api-Key", &self.api_key)
            .send()
            .await
            .map_err(document_server_unreachable)?;

        let status = response.status();
        if !status.is_success() {
            return Err(map_error_body(status, response).await);
        }

        let body: ApiResponse<TemplatesResponse> =
            response.json().await.map_err(document_server_unreachable)?;
        Ok(body
            .data
            .ok_or_else(|| AppError::internal("document-server returned no template list"))?
            .templates)
    }

    /// The built-in default template of a document kind (`x-default`).
    pub async fn default_template(&self, kind: &str) -> AppResult<TemplateInfo> {
        self.list_templates()
            .await?
            .iter()
            .find(|template| template.kind == kind && template.is_default)
            .cloned()
            .ok_or_else(|| {
                AppError::internal(format!(
                    "document-server publishes no default template for kind '{kind}'"
                ))
            })
    }

    /// Renders the built-in default template of a document kind (e.g.
    /// `"analytics-report"`) against `data`; see [`Self::render_template`].
    pub async fn render(&self, kind: &str, data: serde_json::Value) -> AppResult<Vec<u8>> {
        let info = self.default_template(kind).await?;
        self.render_template(&info, data).await
    }

    /// The stored archive of a package the calling tenant may use
    /// (`GET /api/packages/{name}/archive`).
    pub async fn package_archive(&self, name: &str) -> AppResult<Vec<u8>> {
        let url = format!("{}/api/packages/{name}/archive", self.base_url);
        let response = self
            .http
            .get(&url)
            .headers(forwarded_headers())
            .header("X-Internal-Api-Key", &self.api_key)
            .send()
            .await
            .map_err(document_server_unreachable)?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_error_body(status, response).await);
        }
        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(document_server_unreachable)
    }

    /// Renders a specific template's own sample payload — what the Settings
    /// preview shows. The sample's `logoUrl` is dropped: it points at an
    /// external image and a preview must not depend on the network.
    pub async fn render_sample(&self, template: &TemplateInfo) -> AppResult<Vec<u8>> {
        let mut sample = template.sample.clone();
        if let Some(fields) = sample.as_object_mut() {
            fields.remove("logoUrl");
        }
        self.render_template(template, sample).await
    }

    /// Renders `template` against `data` and returns the raw PDF bytes. The
    /// payload is validated against the template's published schema first
    /// when it has one — a builder/template contract drift is caught *here*,
    /// with field-level messages, instead of surfacing as that service's
    /// identical 422 after a wasted round trip (or worse, silently rendering
    /// blanks for a template without server-side validation). `data` becomes
    /// the Typst template's `sys.inputs` — see each `.typ` file's header
    /// comment in `document-server/templates/documents/` for the exact field
    /// contract.
    pub async fn render_template(
        &self,
        info: &TemplateInfo,
        data: serde_json::Value,
    ) -> AppResult<Vec<u8>> {
        validate_payload(&info.data_schema, &data)?;

        let url = format!("{}/api/render/{}", self.base_url, info.key);
        let started = std::time::Instant::now();

        let response = self
            .http
            .post(&url)
            .headers(forwarded_headers())
            .header("X-Internal-Api-Key", &self.api_key)
            .json(&data)
            .send()
            .await
            .map_err(document_server_unreachable)?;

        let status = response.status();
        if !status.is_success() {
            tracing::warn!(
                category = "http",
                event = "outbound",
                service = "document-server",
                url = %url,
                template = %info.name,
                status = status.as_u16(),
                latency_ms = started.elapsed().as_millis() as u64,
                "document-server render failed"
            );
            return Err(map_error_body(status, response).await);
        }

        let bytes = response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(document_server_unreachable)?;
        tracing::info!(
            category = "http",
            event = "outbound",
            service = "document-server",
            url = %url,
            template = %info.name,
            status = status.as_u16(),
            pdf_bytes = bytes.len(),
            latency_ms = started.elapsed().as_millis() as u64,
            "document-server render completed"
        );
        Ok(bytes)
    }
}

/// How many schema violations to include in one rejection message before
/// truncating — enough for an integrator to see the full shape of the
/// problem without a wall of text.
const MAX_REPORTED_SCHEMA_ERRORS: usize = 10;

/// Validates a payload against a published template schema. Same code and
/// status the document-server itself answers with for its own validation
/// (`RENDER_VALIDATION_FAILED`, 422), so callers treat both identically —
/// this is purely a fail-fast mirror of that service's enforcement.
fn validate_payload(data_schema: &serde_json::Value, data: &serde_json::Value) -> AppResult<()> {
    let validator = jsonschema::validator_for(data_schema).map_err(|err| {
        AppError::internal(format!(
            "document-server published an invalid schema for this template: {err}"
        ))
    })?;

    if validator.is_valid(data) {
        return Ok(());
    }

    let violations: Vec<String> = validator
        .iter_errors(data)
        .take(MAX_REPORTED_SCHEMA_ERRORS)
        .map(|err| {
            let path = err.instance_path().to_string();
            if path.is_empty() {
                err.to_string()
            } else {
                format!("at '{path}': {err}")
            }
        })
        .collect();

    Err(AppError::unprocessable_entity(
        "RENDER_VALIDATION_FAILED",
        format!(
            "payload does not match this template's published data schema ({})",
            violations.join("; ")
        ),
    ))
}

/// Carries the current request's id to document-server so its log lines
/// join the same trace as the frontend click and this backend request, plus
/// (in `TENANT_MODE=multi`) the calling tenant. The cloud backend shares one
/// document-server instance across every tenant, so `X-Tenant-Key` is what
/// keeps two tenants' stored shop-profile/bank-detail blobs and rendered
/// documents apart on that side — see document-server's
/// `core::middleware::auth::TenantKey`. `core::tenancy::current_tenant_id()`
/// is `None` in single-shop mode (desktop, self-hosted) or for any call
/// outside a tenant-scoped request, so the header is simply omitted there —
/// document-server's default (`TenantKey::Single`) is exactly today's
/// behaviour, unchanged.
fn forwarded_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(id) = crate::core::logging::request::current_request_id()
        && let Ok(value) = reqwest::header::HeaderValue::from_str(&id)
    {
        headers.insert(crate::core::logging::request::REQUEST_ID_HEADER, value);
    }
    if let Some(tenant_id) = crate::core::tenancy::current_tenant_id()
        && let Ok(value) = reqwest::header::HeaderValue::from_str(&tenant_id)
    {
        headers.insert("X-Tenant-Key", value);
    }
    headers
}

fn document_server_unreachable(err: reqwest::Error) -> AppError {
    AppError::internal(format!("document-server request failed: {err}"))
}

/// Translates a non-2xx document-server response into an `AppError`,
/// preserving its status/code/message where possible rather than collapsing
/// everything to a generic 500 — a 404 `TEMPLATE_NOT_FOUND` or 422
/// `RENDER_VALIDATION_FAILED` from that service should surface as the same
/// status here, not get flattened.
async fn map_error_body(status: reqwest::StatusCode, response: reqwest::Response) -> AppError {
    #[derive(serde::Deserialize)]
    struct DocumentServerError {
        message: String,
        code: String,
    }

    let body = match response.json::<DocumentServerError>().await {
        Ok(body) => body,
        Err(_) => {
            return AppError::internal(format!(
                "document-server returned {status} with an unreadable error body"
            ));
        }
    };

    match status {
        reqwest::StatusCode::NOT_FOUND => AppError::not_found_with_code(body.message, body.code),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
            AppError::unprocessable_entity(body.code, body.message)
        }
        reqwest::StatusCode::UNAUTHORIZED => AppError::internal(format!(
            "document-server rejected our X-Internal-Api-Key ({}): {}",
            body.code, body.message
        )),
        _ => AppError::internal(format!(
            "document-server error ({}): {}",
            body.code, body.message
        )),
    }
}
