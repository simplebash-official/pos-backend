use super::{
    AppEnvironment, Config, ConfigError, Database, IdentityAuth, Provisioning, TenantMode,
    Variables,
};
use std::collections::HashMap;

fn valid() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("DATABASE_TYPE", "sqlite"),
        ("DATABASE_URL", "sqlite://data/pos.db?mode=rwc"),
        ("TENANT_MODE", "single"),
        ("JWT_SECRET", "0123456789abcdef0123456789abcdef"),
        ("JWT_EXPIRY_HOURS", "12"),
        ("PORT", "8080"),
        ("BIND_ADDR", "127.0.0.1"),
        ("DOCUMENT_SERVER_URL", "http://127.0.0.1:8090"),
        ("DOCUMENT_SERVER_API_KEY", "a-real-key"),
        ("GENERATED_DOCUMENTS_DIR", "generated_documents"),
        ("RETURN_WINDOW_DAYS", "30"),
        ("AUTO_SEED", "false"),
        ("CORS_ALLOWED_ORIGINS", ""),
        ("IDENTITY_AUTH", "disabled"),
        ("PROVISIONING", "disabled"),
        ("APP_ENV", "production"),
        ("RATE_LIMIT_BURST", "200"),
        ("RATE_LIMIT_REFILL_PER_SEC", "50"),
        ("RATE_LIMIT_SENSITIVE_BURST", "20"),
        ("RATE_LIMIT_SENSITIVE_REFILL_PER_SEC", "0.5"),
        ("RATE_LIMIT_LIVE_BURST", "400"),
        ("RATE_LIMIT_LIVE_REFILL_PER_SEC", "100"),
        ("LOG_FILTER", "info"),
        ("LOG_FORMAT", "text"),
        ("LOG_BODY_CAP_BYTES", "1024"),
        ("LOG_SQL", "slow"),
        ("LOG_HTTP_BODIES", "false"),
        ("INSTALLATION_ID", "inst_1"),
        ("PLATFORM", "linux"),
        ("SEED_ADMIN_NAME", "Admin"),
        ("SEED_ADMIN_PASSWORD", "a-chosen-password"),
    ])
}

fn parse(variables: &HashMap<&str, &str>) -> Result<Config, ConfigError> {
    Config::from_variables(&Variables::new(&|name| {
        variables.get(name).map(|value| value.to_string())
    }))
}

fn error_of(variables: &HashMap<&str, &str>) -> String {
    parse(variables)
        .expect_err("the configuration is invalid")
        .to_string()
}

#[test]
fn a_complete_configuration_parses() {
    let config = parse(&valid()).expect("valid");
    assert_eq!(config.tenant_mode, TenantMode::Single);
    assert_eq!(config.app_env, AppEnvironment::Production);
    assert!(matches!(config.database, Database::Sqlite { .. }));
    assert_eq!(config.identity_auth, IdentityAuth::Disabled);
    assert_eq!(config.provisioning, Provisioning::Disabled);
    assert!(config.cors_allowed_origins.is_empty());
}

#[test]
fn every_required_variable_is_required() {
    for name in valid().keys() {
        let mut variables = valid();
        variables.remove(name);
        let message = error_of(&variables);
        assert!(message.contains(name), "{name}: {message}");
    }
}

#[test]
fn a_blank_value_counts_as_missing_except_for_the_origin_list() {
    let mut variables = valid();
    variables.insert("JWT_EXPIRY_HOURS", "  ");
    assert!(error_of(&variables).contains("missing"));
    variables.insert("JWT_EXPIRY_HOURS", "12");
    variables.insert("CORS_ALLOWED_ORIGINS", "  ");
    assert!(parse(&variables).is_ok());
}

#[test]
fn mongodb_needs_its_uri_and_name_and_sqlite_does_not() {
    let mut variables = valid();
    variables.insert("DATABASE_TYPE", "mongodb");
    variables.remove("DATABASE_URL");
    assert!(error_of(&variables).contains("MONGODB_URI"));
    variables.insert("MONGODB_URI", "mongodb://localhost");
    assert!(error_of(&variables).contains("MONGODB_DB_NAME"));
    variables.insert("MONGODB_DB_NAME", "pos");
    assert!(matches!(
        parse(&variables).expect("valid").database,
        Database::Mongo { .. }
    ));
}

#[test]
fn multi_tenant_mode_requires_mongodb() {
    let mut variables = valid();
    variables.insert("TENANT_MODE", "multi");
    assert!(error_of(&variables).contains("TENANT_MODE"));
}

#[test]
fn a_flag_must_be_exactly_true_or_false() {
    let mut variables = valid();
    variables.insert("AUTO_SEED", "yes");
    assert!(error_of(&variables).contains("AUTO_SEED"));
}

#[test]
fn a_placeholder_or_short_secret_is_refused() {
    let mut variables = valid();
    variables.insert(
        "JWT_SECRET",
        "replace_with_a_secure_random_64_character_hex_string",
    );
    assert!(error_of(&variables).contains("JWT_SECRET"));
    variables.insert("JWT_SECRET", "short");
    assert!(error_of(&variables).contains("JWT_SECRET"));
}

#[test]
fn identity_auth_enabled_needs_its_settings_and_a_shop_in_single_mode() {
    let mut variables = valid();
    variables.insert("IDENTITY_AUTH", "enabled");
    assert!(error_of(&variables).contains("IDENTITY_JWKS_URL"));
    variables.insert("IDENTITY_JWKS_URL", "https://auth.example/jwks.json");
    assert!(error_of(&variables).contains("IDENTITY_ISSUER"));
    variables.insert("IDENTITY_ISSUER", "issuer");
    assert!(error_of(&variables).contains("IDENTITY_TENANT_ID"));
    variables.insert("IDENTITY_TENANT_ID", "tnt_1");
    assert_eq!(
        parse(&variables).expect("valid").identity_auth,
        IdentityAuth::Enabled {
            jwks_url: "https://auth.example/jwks.json".to_string(),
            issuer: "issuer".to_string(),
            tenant_id: Some("tnt_1".to_string()),
        }
    );
}

#[test]
fn provisioning_needs_multi_tenant_mode_and_a_long_secret() {
    let mut variables = valid();
    variables.insert("PROVISIONING", "enabled");
    assert!(error_of(&variables).contains("PROVISIONING"));

    variables.insert("DATABASE_TYPE", "mongodb");
    variables.insert("MONGODB_URI", "mongodb://localhost");
    variables.insert("MONGODB_DB_NAME", "pos");
    variables.insert("TENANT_MODE", "multi");
    assert!(error_of(&variables).contains("PROVISION_SECRET"));
    variables.insert("PROVISION_SECRET", "too-short");
    assert!(error_of(&variables).contains("PROVISION_SECRET"));
    variables.insert("PROVISION_SECRET", "a-secret-of-16+chars");
    assert!(matches!(
        parse(&variables).expect("valid").provisioning,
        Provisioning::Enabled { .. }
    ));
}

#[test]
fn a_public_instance_refuses_to_seed_a_well_known_admin_password() {
    let mut variables = valid();
    variables.insert("BIND_ADDR", "0.0.0.0");
    variables.insert("AUTO_SEED", "true");
    variables.insert("SEED_ADMIN_PASSWORD", "admin@1234");
    assert!(error_of(&variables).contains("SEED_ADMIN_PASSWORD"));
    // On loopback the desktop may use any password.
    variables.insert("BIND_ADDR", "127.0.0.1");
    assert!(parse(&variables).is_ok());
}

#[test]
fn cors_origins_are_listed_trimmed_and_checked() {
    let mut variables = valid();
    variables.insert(
        "CORS_ALLOWED_ORIGINS",
        " https://a.example/ , http://b.example ",
    );
    assert_eq!(
        parse(&variables).expect("valid").cors_allowed_origins,
        ["https://a.example", "http://b.example"]
    );
    variables.insert("CORS_ALLOWED_ORIGINS", "bad\u{7f}origin");
    assert!(error_of(&variables).contains("CORS_ALLOWED_ORIGINS"));
}

#[test]
fn an_invalid_log_filter_or_negative_rate_limit_is_refused() {
    let mut variables = valid();
    variables.insert("LOG_FILTER", "[[[");
    assert!(error_of(&variables).contains("LOG_FILTER"));
    variables.insert("LOG_FILTER", "info");
    variables.insert("RATE_LIMIT_BURST", "-1");
    assert!(error_of(&variables).contains("RATE_LIMIT_BURST"));
}

#[test]
fn the_debug_output_hides_every_secret() {
    let mut variables = valid();
    variables.insert("JWT_SECRET", "super-secret-jwt-value-0123456789abcdef");
    variables.insert("DOCUMENT_SERVER_API_KEY", "super-secret-api-key");
    variables.insert("SEED_ADMIN_PASSWORD", "super-secret-password");
    let shown = format!("{:?}", parse(&variables).expect("valid"));
    assert!(!shown.contains("super-secret"), "{shown}");
}
