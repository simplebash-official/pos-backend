//! Brevo transactional email client for password reset emails.
//!
//! Dispatches single password reset template emails via the Brevo SMTP API.
//! Sends are not idempotent (Brevo queues the message), so failures are reported
//! without automatic retry.

use std::time::Duration;

use serde_json::json;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(15);

/// Errors that can occur when sending a transactional email through Brevo.
#[derive(Debug, thiserror::Error)]
pub enum BrevoError {
    #[error("request failed: {0}")]
    Transport(String),
    #[error("Brevo answered {status}: {message}")]
    Rejected { status: u16, message: String },
    #[error("Brevo accepted the request but returned no messageId")]
    NoMessageId,
}

/// HTTP client for interacting with the Brevo v3 SMTP API.
#[derive(Clone)]
pub struct BrevoClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    reset_template_id: i64,
}

impl std::fmt::Debug for BrevoClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrevoClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("reset_template_id", &self.reset_template_id)
            .finish()
    }
}

impl BrevoClient {
    /// Constructs a new `BrevoClient` with preconfigured timeouts.
    pub fn new(base_url: String, api_key: String, reset_template_id: i64) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .build()
            .expect("reqwest client with static settings");
        Self {
            http,
            base_url,
            api_key,
            reset_template_id,
        }
    }

    /// Sends the password reset template to `to_email`; returns Brevo's `messageId`.
    pub async fn send_password_reset(
        &self,
        to_email: &str,
        to_name: &str,
        reset_link: &str,
        shop_code: Option<&str>,
    ) -> Result<String, BrevoError> {
        let mut params = serde_json::Map::new();
        params.insert("name".to_string(), json!(to_name));
        params.insert("reset_link".to_string(), json!(reset_link));
        params.insert("resetLink".to_string(), json!(reset_link));
        if let Some(code) = shop_code {
            params.insert("shop_code".to_string(), json!(code));
            params.insert("shopCode".to_string(), json!(code));
        }

        let response = self
            .http
            .post(format!("{}/v3/smtp/email", self.base_url))
            .header("api-key", &self.api_key)
            .header("accept", "application/json")
            .json(&json!({
                "to": [{ "email": to_email, "name": to_name }],
                "templateId": self.reset_template_id,
                "params": serde_json::Value::Object(params),
            }))
            .send()
            .await
            .map_err(|e| BrevoError::Transport(e.to_string()))?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| BrevoError::Transport(format!("unreadable body: {e}")))?;

        if !status.is_success() {
            return Err(BrevoError::Rejected {
                status: status.as_u16(),
                message: body["message"].as_str().unwrap_or("no message").to_string(),
            });
        }

        body["messageId"]
            .as_str()
            .map(str::to_string)
            .ok_or(BrevoError::NoMessageId)
    }
}
