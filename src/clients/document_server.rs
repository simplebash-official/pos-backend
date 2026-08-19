// HTTP client for the sibling document-server (Typst PDF rendering,
// `../document-server`). This is the *only* caller of that service anywhere
// in the system — the frontend never talks to it directly (see
// `modules::documents::service`, which is the only module allowed to call
// this client). Built once at startup and shared read-only via `AppState`,
// the same "connect/build once, hand out a shared handle" shape as
// `clients::mongo::connect`.

use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::core::{
    error::{AppError, AppResult},
    response::ApiResponse,
};

pub struct DocumentServerClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    // Template name -> `tpl_<nanoid>` key, populated lazily from
    // `GET /api/templates` on first use. Template keys are stable for the
    // lifetime of a document-server database (re-syncing a template on disk
    // upserts by name, never regenerating the key — see that service's
    // `sync_templates_from_disk`), so caching avoids an extra round trip on
    // every render without ever going stale in practice; a process restart
    // clears the cache anyway.
    template_key_cache: RwLock<HashMap<String, String>>,
}

#[derive(serde::Deserialize)]
struct TemplateSummary {
    key: String,
    name: String,
}

#[derive(serde::Deserialize)]
struct TemplatesResponse {
    templates: Vec<TemplateSummary>,
}

impl DocumentServerClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            api_key,
            template_key_cache: RwLock::new(HashMap::new()),
        }
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

    /// Resolves a template's stable name (e.g. `"a4-invoice"`) to the
    /// `templates.key` `POST /api/render/{templateKey}` expects. Unauthenticated
    /// on document-server's side (`templates` routes are deliberately left
    /// open — see that service's Phase 1 auth notes), so no header here.
    async fn resolve_template_key(&self, template_name: &str) -> AppResult<String> {
        if let Some(key) = self.template_key_cache.read().await.get(template_name) {
            return Ok(key.clone());
        }

        let url = format!("{}/api/templates", self.base_url);
        let response = self
            .http
            .get(&url)
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

        let mut cache = self.template_key_cache.write().await;
        for template in &templates {
            cache.insert(template.name.clone(), template.key.clone());
        }

        cache.get(template_name).cloned().ok_or_else(|| {
            AppError::internal(format!(
                "document-server has no template named '{template_name}'"
            ))
        })
    }

    /// Renders `template_name` against `data` and returns the raw PDF
    /// bytes. `data` becomes the Typst template's `sys.inputs` — see each
    /// `.typ` file's header comment in `document-server/templates/documents/`
    /// for the exact field contract.
    pub async fn render(&self, template_name: &str, data: serde_json::Value) -> AppResult<Vec<u8>> {
        let template_key = self.resolve_template_key(template_name).await?;
        let url = format!("{}/api/render/{}", self.base_url, template_key);

        let response = self
            .http
            .post(&url)
            .header("X-Internal-Api-Key", &self.api_key)
            .json(&data)
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
