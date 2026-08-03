# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

The Rust/Axum API backend for **jana2u-pos**, a point-of-sale system for a repair/retail shop (billing, repairs, print jobs, inventory, customers, reports). Persistence is MongoDB. The sibling `../frontend` (React + Vite + Mantine) is the client this API serves, expecting endpoints under `env.apiBaseUrl` (`/api` by default).

## Commands

- `cargo build` — compile
- `cargo run` — run the server (equivalent to `cargo run --bin jana2u_pos_backend`; required explicitly if disambiguating from the seed binaries)
- `cargo test` — run all tests (`tests/scenarios_test.rs` and `tests/inventory_test.rs` require a reachable MongoDB — see below; `tests/openapi_test.rs` and `tests/response_format_test.rs` do not)
- `cargo test --test scenarios_test health_route_returns_success_format` — run a single integration test by name
- `cargo fmt` / `cargo fmt --check` — format / verify formatting
- `cargo run --bin seed_api_key` — generate and insert a random API key into the `api_keys` collection
- `cargo run --bin seed_providers` — upserts the inventory module's default category/subcategory reference data into the `categories` collection (see `modules::inventory::categories::default_categories`)

**After every change**, run the following and fix anything it reports before considering the work done:

```
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo check && cargo test && cargo build --release
```

(`cargo fmt --check` fails on unformatted code without rewriting it; `clippy -D warnings` treats every lint as an error; `cargo check` is the fast type/borrow check; `cargo build --release` is the pre-deploy build. `rustfmt`/`clippy` components must be installed — `rustup component add rustfmt clippy` — if either command is missing.)

**Every change must be tested — this is mandatory, not just running the pipeline above.** `cargo test` in that pipeline only re-runs whatever tests already exist; it does not by itself prove a new change works. So for any change:
1. Find the test file that already covers the part of the code you're touching (see the three `tests/*.rs` files below) and extend it with a case for the change.
2. If no test file covers that part yet, **create one** — follow the existing naming (`tests/<area>_test.rs`) and pick the right style: no-Mongo unit/integration style (`tests/response_format_test.rs`, `tests/openapi_test.rs`) for anything that doesn't need real data, or the full-stack style (`tests/scenarios_test.rs` via `tests/common::spawn_app()`) for anything that does.
3. Run it and confirm it passes before considering the change done.

Config is loaded from `.env` (via `dotenvy`) in every binary and integration test. Copy `.env.example` to `.env` and point `MONGODB_URI` at a running MongoDB instance before running or testing — `Config::from_env()` fails fast (process exits) on missing/invalid vars, and the Mongo client pings the server at startup, so a bad connection string is caught immediately rather than on first request.

Three integration test files, each a different tradeoff between speed and realism:
- `tests/scenarios_test.rs` (+ `tests/common/mod.rs::spawn_app()`) — builds the real router against a **real** MongoDB (no mocking), using `MONGODB_TEST_DB_NAME` (defaults to `jana2u_pos_test`) instead of the dev database so it never touches dev data. Use for anything that actually reads/writes Mongo.
- `tests/openapi_test.rs` — builds the real router with a `mongodb::Client` that is never pinged (still needs `MONGODB_URI` to be a syntactically valid connection string via `.env`, but no live Mongo needed). Asserts the OpenAPI spec lists every module path and Swagger UI serves. Use for anything about routing/docs wiring rather than data.
- `tests/response_format_test.rs` — no router, no Mongo at all; calls `core::response::ApiResponse`/`core::error::AppError` directly and asserts the JSON shape. Use for anything about the response/error envelope itself.

## Architecture

**Binary/library split**: `src/lib.rs` re-exports `app`, `clients`, `core`, `domain`, `modules`, `workers` as a library crate. `src/main.rs` is a thin binary that wires config → Mongo → router → serve. This split exists so `src/bin/*.rs` (seed scripts) and `tests/*.rs` (integration tests) can reuse the same modules via `jana2u_pos_backend::...` — Cargo integration tests and extra `src/bin/` binaries are separate crates and cannot use `crate::` paths from `main.rs`.

**Request flow**: `main.rs` calls `Config::from_env()` (`core/config/env.rs`) → `clients::mongo::connect()` → builds `AppState { config: Arc<Config>, db: Database }` (defined in `app.rs`) → `app::build_router(state)`. `app.rs` builds an `OpenApiRouter` (nesting each feature module's router under `/api/<module>`, e.g. `/api/billing`, `/api/print-jobs`), splits it into an `axum::Router` + `utoipa::openapi::OpenApi`, merges in Swagger UI, then applies `CorsLayer` (all origins allowed) and `TraceLayer`.

**Request logging**: every request is printed to the terminal (method, URI, status, latency) with no setup needed — `main.rs` sets a default `EnvFilter` of `jana2u_pos_backend=info,tower_http=info,info` (overridable via `RUST_LOG`), and `app.rs`'s `TraceLayer` is explicitly configured with `DefaultMakeSpan`/`DefaultOnRequest`/`DefaultOnResponse` at `Level::INFO` — tower_http's own defaults for those three are `DEBUG`, which would stay silent under an `info`-level filter. If this layer is ever touched, keep it at `INFO` (or lower the filter instead) so request activity doesn't silently disappear.

**Feature modules** (`src/modules/<name>/`): `auth`, `billing`, `customers`, `inventory`, `print_jobs`, `repairs`, `reports` — one per frontend feature. Each has `mod.rs` + `routes.rs` exporting `pub fn router() -> OpenApiRouter<AppState>`; most still only have a placeholder `GET /` status route. `inventory` is built out as the reference example: `routes.rs` has full product CRUD (list/get/create/update/delete, batch delete, stock adjustment + movement history, low-stock) plus full category/subcategory management (read endpoints for everyone; create/rename/delete a category and add/remove a subcategory gated behind `AdminUser` — see `core/middleware/auth.rs`) backed by a `categories` collection (seeded via `cargo run --bin seed_providers`). Renaming or deleting a category cascades/guards against `products` referencing it (rename updates matching products' `category` field; delete and subcategory-removal are blocked with a 409 while still referenced) rather than leaving orphaned references — apply the same cascade/guard discipline to any other module that owns reference data products/other records point to by name. `model.rs` holds its Mongo document shapes (kept separate from `domain::inventory`'s API-facing types). See **API docs** below for the required pattern when adding routes, and use `inventory` as the template for building out another module.

**Route File Organization**: Every module's `routes.rs` MUST follow a clean, structured ordering using section banners (`// ============================================================================`) in this exact sequence:
1. `// Router`: `pub fn router() -> OpenApiRouter<AppState>` with `routes!(...)` calls grouped and commented by feature to mirror the file layout.
2. `// Helpers`: Private helper functions for that module (e.g. `parse_object_id`, whitelist mapping, numeric validation).
3. `// <Feature Sections>`: Grouped feature handler blocks (e.g., `// Module status`, `// Products`, `// Stock`, `// Categories`, `// Subcategories`). Read/validation endpoints (such as `/categories/valid`) belong inside their primary domain section next to other reads rather than at the bottom of the file.

**Custom Prefixed Unique Model Keys**: Every MongoDB collection document model MUST include a custom unique `key: String` alongside `_id: ObjectId`. Keys follow the format `<prefix>_<nanoid>` (e.g. `prod_x7K9mP2...` for products, `cat_B8nL1q...` for categories, `sm_9pX2kQ...` for stock movements, `ak_...` for API keys). Generated via `crate::core::id::generate_id(prefix)` (defined in `core/id.rs`, backed by `nanoid!(16)`). From now onwards, all models, seed scripts, and API domain response payloads MUST include and maintain this `key` field.

**`core/`** — cross-cutting infrastructure, not domain logic:
- `core/config/env.rs` — `Config::from_env()` parses all required env vars once at startup and fails fast (`MONGODB_URI`, `MONGODB_DB_NAME`, `JWT_SECRET` required; `PORT` defaults to 8080).
- `core/middleware/auth.rs` — `CurrentUser` is an Axum extractor (`FromRequestParts<AppState>`) that verifies the `Authorization: Bearer <jwt>` header with `jsonwebtoken` using `Config::jwt_secret`. Add `CurrentUser` as a handler argument to require auth on a route; omit it to keep a route public. `AdminUser` wraps `CurrentUser` and additionally requires the JWT's `role` claim to be `"admin"` (403 `ADMIN_REQUIRED` otherwise) — it's the reusable pattern for **any** admin-only endpoint across modules, not just inventory's category management (its first user); add `AdminUser` as a handler argument the same way, and mark the route `security(("bearerAuth" = []))` in its `#[utoipa::path]`. There is deliberately no tenant/org-membership middleware yet — this is a single-shop deployment for now. Note `jsonwebtoken` needs its `rust_crypto` feature enabled in `Cargo.toml` (see comment there) — without it, `encode`/`decode` panic at runtime with no crypto backend installed.
- `core/error.rs` — `AppError` (`NotFound`, `Validation`, `Unauthorized`, `Forbidden`, `Internal` — each `{ message, code: Option<String> }` — plus `Custom { status, code, message }` for anything else) implements `IntoResponse`, serializing to `core::response::ErrorResponse` (`{ success: false, message, code, statusCode }`, camelCase) with the matching HTTP status; also `From<mongodb::error::Error>` / `From<jsonwebtoken::errors::Error>`, so handlers can `?`-propagate Mongo/JWT failures directly. Use `AppResult<T>` as the handler return type. Construct via `AppError::not_found("msg")` / `AppError::not_found_with_code("msg", "SOME_CODE")` (same pattern for `validation`/`unauthorized`/`forbidden`/`internal`), or `AppError::custom(status, "CODE", "msg")` for anything outside those five. A missing `code` on the non-`Custom` variants defaults to the upper-snake-case of the variant name (e.g. `NOT_FOUND`).
- `core/response.rs` — `ApiResponse<T>` is the success envelope every handler should return: `{ success: true, data: Option<T>, message: Option<String> }` (both fields omitted from JSON when `None`), built via `ApiResponse::success(data, message)` / `::data(data)` / `::message(message)`. Its `IntoResponse` impl **always returns HTTP 200** — a handler returning `Json<ApiResponse<T>>` directly (as opposed to `AppResult<Json<ApiResponse<T>>>` returning `Err(AppError::...)`) cannot produce a non-2xx status; failure cases must go through `AppError`, not `ApiResponse`. Also defines `ErrorResponse`, the JSON shape `AppError::into_response` produces (see above) — don't construct `ErrorResponse` directly outside `core/error.rs`.
- `core/constants/` — central source of truth for constants across the backend: `modules.rs` (feature module names), `codes.rs` (standardized error and status codes), `http_status.rs` (re-exported HTTP status codes), and `prefixes.rs` (model key prefix constants like `PRODUCT`, `CATEGORY`, `STOCK_MOVEMENT`, `API_KEY`).
- `core/utils.rs` — reusable cross-cutting helpers for route handlers (`module_status_response`, `parse_object_id`, `regex_escape`, `build_bson_regex`, `calculate_pagination`).
- `core/openapi.rs` — `ApiDoc` (`#[derive(utoipa::OpenApi)]`) holds only top-level metadata (title/description/version, tags, the `bearerAuth` security scheme); actual paths are collected from the modules at router-build time, not listed here.

**`domain/`** — pure business types shared across modules (no I/O, no Axum/Mongo types beyond `serde`/`utoipa` derives). Currently holds `HealthResponse`/`ModuleStatusResponse`, the inner `data` payloads that route handlers wrap in `core::response::ApiResponse<T>` before returning (e.g. `Json<ApiResponse<ModuleStatusResponse>>`, not the raw type) — populate further as modules grow instead of putting business logic directly in `routes.rs`.

**`clients/mongo.rs`** — `connect(uri, db_name)` builds the `mongodb::Client`, pings the target database, and returns a `Database` handle. This is the only place Mongo connection setup happens; modules access `AppState.db` rather than reconnecting. `ClientOptions::parse` is explicitly given `ResolverConfig::cloudflare()` rather than the OS default — on some hosts (observed on macOS with a link-local IPv6 nameserver like `fe80::...%en0`) the driver's built-in resolver fails to parse the system DNS config, which breaks `mongodb+srv://` SRV/TXT lookups with a `DnsResolve` error even though the URI and credentials are correct. Any other place a `ClientOptions` gets built directly from a URI (e.g. `tests/openapi_test.rs`) needs the same `.resolver_config(ResolverConfig::cloudflare())` call for the same reason.

**`workers/`** — placeholder for background jobs (e.g. scheduled reports, print queue processing); empty so far.

**`src/bin/`** — standalone binaries sharing the lib's config/Mongo plumbing, run via `cargo run --bin <name>`, not part of the HTTP server.

## API docs (OpenAPI/Swagger)

Swagger UI: `http://localhost:8080/docs`. Raw OpenAPI 3.1 JSON: `http://localhost:8080/api-docs/openapi.json`. Generated via `utoipa` + `utoipa-axum` + `utoipa-swagger-ui` — there is no hand-written spec file to fall out of sync.

**The docs are only correct if every route follows this pattern** (see any `modules/*/routes.rs` for a working example):
1. Annotate the handler with `#[utoipa::path(get, path = "/foo", tag = "<module>", responses((status = 200, body = ApiResponse<SomeResponseType>)))]` (also add `request_body = ...` / path or query params as needed, and `security(("bearerAuth" = []))` if the route requires `CurrentUser`). Success responses are wrapped in `core::response::ApiResponse<T>`, not the bare domain type — see `core/response.rs` above.
2. Response/request payload types (the `T` inside `ApiResponse<T>`, and any request body type) need `#[derive(utoipa::ToSchema)]` alongside `serde::Serialize`/`Deserialize`.
3. Register the handler in that module's `router()` via `OpenApiRouter::new().routes(routes!(handler_one, handler_two, ...))` — **never** mount a handler with plain `axum::routing::get/post/...` inside a module's `router()`, since only handlers passed through the `routes!()` macro get collected into the OpenAPI document.

`tests/openapi_test.rs` asserts `/api-docs/openapi.json` contains a path for every module and that `/docs/` serves — run it after adding a module or route to catch a handler that was wired with plain `axum::routing` instead of `routes!()`.
