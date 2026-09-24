use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

/// Root OpenAPI document. Paths are populated at router build time by
/// merging each module's `OpenApiRouter` (see `app::build_router`) — this
/// struct only carries top-level metadata and shared components.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(
        title = "SimpleBash POS API",
        description = "POS backend for a repair/retail shop: billing, repairs, print jobs, inventory, customers, reports.",
        version = "0.1.0"
    ),
    tags(
        (name = "auth", description = "Authentication"),
        (name = "billing", description = "Billing"),
        (name = "customers", description = "Customers"),
        (name = "employees", description = "Employee HR/commission profiles"),
        (name = "inventory", description = "Inventory"),
        (name = "print_jobs", description = "Print jobs"),
        (name = "repairs", description = "Repairs"),
        (name = "reports", description = "Reports"),
        (name = "suppliers", description = "Suppliers"),
        (name = "supplier_products", description = "Supplier-product links"),
        (name = "purchases", description = "Supplier purchase/stock-intake history"),
        (name = "users", description = "User accounts & role management"),
        (name = "backup", description = "Data backup and restore"),
        (name = "system", description = "Initial installation, onboarding and system setup"),
        (name = "tenants", description = "Server-to-server shop provisioning (identity service only)"),
    )
)]
pub struct ApiDoc;

/// Registers the `bearerAuth` security scheme referenced by every route's
/// `security(("bearerAuth" = []))` annotation. Without this, `utoipa` would
/// emit those annotations pointing at a scheme that doesn't exist in the
/// generated document, and Swagger UI's "Authorize" button would have
/// nothing to configure.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearerAuth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}
