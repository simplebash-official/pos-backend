use simplebash_pos_backend::core::config::{Config, Variables};
use std::collections::HashMap;
use std::sync::Mutex;

/// Overrides a test binary applies to every configuration it builds.
static BINARY_OVERRIDES: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());

/// Makes every `test_config` of this test binary use `value` for `name`; call it before building
/// an app (for example to lift the rate limit for a stress test).
#[allow(dead_code)]
pub fn set_config_override(name: &'static str, value: &str) {
    BINARY_OVERRIDES
        .lock()
        .expect("config overrides mutex poisoned")
        .push((name, value.to_string()));
}

/// The Atlas connection string every Mongo-backed test runs against (`make check-atlas` sets it).
#[allow(dead_code)]
pub fn mongo_uri() -> String {
    std::env::var("MONGODB_URI")
        .expect("Mongo-backed tests need MONGODB_URI (use `make check-atlas`)")
}

/// A complete, valid test configuration. `overrides` replace individual variables by name, so a
/// test states exactly what it changes and nothing is read from the ambient environment.
pub fn test_config(overrides: &[(&str, &str)]) -> Config {
    let mut variables: HashMap<&str, &str> = HashMap::from([
        ("DATABASE_TYPE", "sqlite"),
        ("DATABASE_URL", "sqlite::memory:"),
        ("TENANT_MODE", "single"),
        (
            "JWT_SECRET",
            "test-jwt-secret-key-that-is-at-least-32-characters-long",
        ),
        ("JWT_EXPIRY_HOURS", "12"),
        ("PORT", "8080"),
        ("BIND_ADDR", "127.0.0.1"),
        ("DOCUMENT_SERVER_URL", "http://127.0.0.1:8090"),
        ("DOCUMENT_SERVER_API_KEY", "test-document-server-key"),
        ("GENERATED_DOCUMENTS_DIR", "generated_documents"),
        ("RETURN_WINDOW_DAYS", "30"),
        ("AUTO_SEED", "false"),
        ("CORS_ALLOWED_ORIGINS", ""),
        ("IDENTITY_AUTH", "disabled"),
        ("PROVISIONING", "disabled"),
        ("APP_ENV", "development"),
        ("RATE_LIMIT_BURST", "200"),
        ("RATE_LIMIT_REFILL_PER_SEC", "50"),
        ("RATE_LIMIT_SENSITIVE_BURST", "20"),
        ("RATE_LIMIT_SENSITIVE_REFILL_PER_SEC", "0.5"),
        ("RATE_LIMIT_LIVE_BURST", "400"),
        ("RATE_LIMIT_LIVE_REFILL_PER_SEC", "100"),
        ("LOG_FILTER", "warn"),
        ("LOG_FORMAT", "text"),
        ("LOG_BODY_CAP_BYTES", "32768"),
        ("LOG_SQL", "slow"),
        ("LOG_HTTP_BODIES", "false"),
        ("INSTALLATION_ID", "inst_test"),
        ("PLATFORM", "test"),
        ("SEED_ADMIN_NAME", "System Admin"),
        ("SEED_ADMIN_PASSWORD", "admin@1234"),
    ]);
    let binary_overrides = BINARY_OVERRIDES
        .lock()
        .expect("config overrides mutex poisoned")
        .clone();
    for (name, value) in &binary_overrides {
        variables.insert(name, value);
    }
    for (name, value) in overrides {
        variables.insert(name, value);
    }
    Config::from_variables(&Variables::new(&|name| {
        variables.get(name).map(|value| value.to_string())
    }))
    .expect("the test configuration is valid")
}

/// The overrides that select a throwaway MongoDB database named `name`.
#[allow(dead_code)]
pub fn mongo_overrides<'a>(uri: &'a str, name: &'a str) -> [(&'static str, &'a str); 3] {
    [
        ("DATABASE_TYPE", "mongodb"),
        ("MONGODB_URI", uri),
        ("MONGODB_DB_NAME", name),
    ]
}
