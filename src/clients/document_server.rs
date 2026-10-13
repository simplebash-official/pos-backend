// HTTP client for the sibling document-server (Typst PDF rendering,
// `../document-server`). This is the *only* caller of that service anywhere
// in the system — the frontend never talks to it directly (see
// `modules::documents::service`, which is the only module allowed to call
// this client). Built once at startup and shared read-only via `AppState`,
// the same "connect/build once, hand out a shared handle" shape as
// `clients::mongo::connect`.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;

use crate::core::{
    error::{AppError, AppResult},
    response::ApiResponse,
};

/// Stable call-site slug -> the human label document-server publishes as a
/// template's `description` (its schema `title`). Its on-disk `name` is an
/// opaque `doc_temp_<nanoid>`, so slugs resolve through the title; this is
/// the **built-in default** template of each kind, and what
/// `generated_documents.template_name` stores.
const TEMPLATE_DESCRIPTIONS: &[(&str, &str)] = &[
    ("a4-invoice", "A4 Invoice"),
    ("thermal-receipt", "Thermal Receipt"),
    ("credit-note", "Credit Note"),
    ("quotation", "Quotation"),
    ("analytics-report", "Analytics Report"),
];

/// The built-in default template's title for a call-site slug, if it has one.
pub fn default_title(slug: &str) -> Option<&'static str> {
    TEMPLATE_DESCRIPTIONS
        .iter()
        .find(|(s, _)| *s == slug)
        .map(|(_, d)| *d)
}

/// Resolves a call-site slug to the `description` document-server publishes.
/// An unrecognised value is returned unchanged, so a caller that already
/// holds a real description still works.
fn slug_to_description(slug: &str) -> &str {
    default_title(slug).unwrap_or(slug)
}

/// How long a fetched template list is trusted. Templates are added and
/// edited on the document-server without a restart (`POST /api/templates/sync`),
/// so the list must refresh by itself; a minute keeps the per-print cost at
/// zero while a new template shows up almost immediately.
const TEMPLATE_CACHE_TTL: Duration = Duration::from_secs(60);

/// The schema keyword a template uses to declare what it is for (see
/// `document-server/templates/CLAUDE.md`).
const KIND_KEYWORD: &str = "x-document-kind";

/// One **active** template as the document-server publishes it.
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    /// The document-server-minted unique key (`tpl_...`) — persisted onto
    /// `generated_documents` rows so the POS side durably holds the ID the
    /// document server assigned. Differs per document-server database.
    pub key: String,
    /// The opaque, stable template id (`doc_temp_<nanoid>`, the `.typ` file
    /// stem). Identical on every deployment, so this is what shops store.
    pub name: String,
    /// Human label (the schema `title`) shown in the dropdowns.
    pub title: String,
    /// What the template is for (`x-document-kind` in its schema), e.g.
    /// `a4-invoice`. `None` for a template that does not declare one.
    pub kind: Option<String>,
    /// JSON Schema for the template's render payload (`dataSchema`), when
    /// the template ships one. `None` → no pre-validation (matches that
    /// service's render-time behavior).
    pub data_schema: Option<serde_json::Value>,
    /// The template's worked sample payload (`data`), used for previews.
    pub sample: Option<serde_json::Value>,
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
    // The active templates from `GET /api/templates`, refreshed after
    // `cache_ttl`. If a refresh fails while a previous list exists, that list
    // keeps being served (with a warning): a document-server hiccup must not
    // stop the shop from printing. The schema rides along in the same entry
    // because `render()` validates every payload against it *before*
    // sending — the schema the document-server publishes is the one contract
    // this backend codes its payload builders against.
    template_cache: RwLock<Option<TemplateCache>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateSummary {
    key: String,
    /// The opaque `doc_temp_<nanoid>` id.
    name: String,
    /// Human label (the template's schema `title`).
    #[serde(default)]
    description: Option<String>,
    /// `GET /api/templates` also lists templates whose `.typ` vanished from
    /// disk (deactivated, never deleted); those must not be offered or used.
    #[serde(default = "default_true")]
    is_active: bool,
    data_schema: Option<serde_json::Value>,
    data: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

#[derive(serde::Deserialize)]
struct TemplatesResponse {
    templates: Vec<TemplateSummary>,
}

impl TemplateSummary {
    fn into_info(self) -> TemplateInfo {
        let kind = self
            .data_schema
            .as_ref()
            .and_then(|schema| schema.get(KIND_KEYWORD))
            .and_then(|kind| kind.as_str())
            .filter(|kind| !kind.is_empty())
            .map(str::to_string);
        TemplateInfo {
            title: self.description.unwrap_or_else(|| self.name.clone()),
            key: self.key,
            name: self.name,
            kind,
            data_schema: self.data_schema,
            sample: self.data,
        }
    }
}

impl DocumentServerClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            api_key,
            cache_ttl: TEMPLATE_CACHE_TTL,
            template_cache: RwLock::new(None),
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

    /// Every **active** template, from the cache while it is fresh, else
    /// from `GET /api/templates`. Unauthenticated on document-server's side
    /// (`templates` routes are deliberately left open — see that service's
    /// auth notes), so no API-key header here.
    pub async fn list_templates(&self) -> AppResult<Arc<Vec<TemplateInfo>>> {
        if let Some(cache) = self.template_cache.read().await.as_ref()
            && cache.loaded_at.elapsed() < self.cache_ttl
        {
            return Ok(cache.templates.clone());
        }

        match self.fetch_templates().await {
            Ok(templates) => {
                let templates = Arc::new(templates);
                *self.template_cache.write().await = Some(TemplateCache {
                    loaded_at: Instant::now(),
                    templates: templates.clone(),
                });
                Ok(templates)
            }
            Err(err) => {
                if let Some(cache) = self.template_cache.read().await.as_ref() {
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
            .send()
            .await
            .map_err(document_server_unreachable)?;

        let status = response.status();
        if !status.is_success() {
            return Err(map_error_body(status, response).await);
        }

        let body: ApiResponse<TemplatesResponse> =
            response.json().await.map_err(document_server_unreachable)?;
        let templates = body
            .data
            .ok_or_else(|| AppError::internal("document-server returned no template list"))?
            .templates;

        Ok(templates
            .into_iter()
            .filter(|template| template.is_active)
            .map(TemplateSummary::into_info)
            .collect())
    }

    /// The active template with this opaque `name` (`doc_temp_<nanoid>`).
    pub async fn find_by_name(&self, name: &str) -> AppResult<Option<TemplateInfo>> {
        Ok(self
            .list_templates()
            .await?
            .iter()
            .find(|template| template.name == name)
            .cloned())
    }

    /// The template a call-site slug (e.g. `"a4-invoice"`) stands for: the
    /// active template published under that slug's built-in title.
    async fn get_template_info(&self, template_slug: &str) -> AppResult<TemplateInfo> {
        let description = slug_to_description(template_slug);
        self.list_templates()
            .await?
            .iter()
            .find(|template| template.title == description)
            .cloned()
            .ok_or_else(|| {
                AppError::internal(format!(
                    "document-server has no template described as '{description}' (slug '{template_slug}')"
                ))
            })
    }

    /// Renders the built-in template of a call-site slug (e.g.
    /// `"analytics-report"`) against `data`; see [`Self::render_template`].
    pub async fn render(&self, template_slug: &str, data: serde_json::Value) -> AppResult<Vec<u8>> {
        let info = self.get_template_info(template_slug).await?;
        self.render_template(&info, data).await
    }

    /// Renders a specific template's own sample payload — what the Settings
    /// preview shows. The sample's `logoUrl` is dropped: it points at an
    /// external image and a preview must not depend on the network.
    pub async fn render_sample(&self, template: &TemplateInfo) -> AppResult<Vec<u8>> {
        let mut sample = template.sample.clone().ok_or_else(|| {
            AppError::not_found_with_code(
                format!(
                    "template '{}' has no sample data to preview",
                    template.title
                ),
                "TEMPLATE_SAMPLE_MISSING",
            )
        })?;
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
        validate_payload(info.data_schema.as_ref(), &data)?;

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
fn validate_payload(
    data_schema: Option<&serde_json::Value>,
    data: &serde_json::Value,
) -> AppResult<()> {
    let Some(data_schema) = data_schema else {
        return Ok(());
    };

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
