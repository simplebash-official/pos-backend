# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

The Rust/Axum API backend for **jana2u-pos**, a point-of-sale system for a repair/retail shop (billing, repairs, print jobs, inventory, customers, reports, suppliers). Persistence is MongoDB. The sibling `../frontend` (React + Vite + Mantine) is the client this API serves, expecting endpoints under `env.apiBaseUrl` (`/api` by default).

## Where the rest of the guidance lives

Architecture, code-writing conventions, and module-specific rules are split into directory-scoped
`CLAUDE.md` files that load automatically when Claude works under those trees — keeping them out of
context for doc-only / config sessions:

- **`src/CLAUDE.md`** — architecture (binary/library split, request flow, every feature module's
  layering and rationale, `core/`/`domain/`/`clients/`), MongoDB aggregation-pipeline policy, and
  the dashboard-stats-endpoint pattern.
- **`src/modules/CLAUDE.md`** — the required pattern for adding/changing routes (OpenAPI/`utoipa`
  wiring) and the rule that `postman/backend.postman_collection.json` is updated in the same change.
- **`src/modules/sync/CLAUDE.md`** — the offline-sync module's byte-identical-DTO rule, cursors,
  soft-delete/tombstone semantics, and idempotency/version-conflict middleware.

The invariants below apply everywhere and are easy to violate, so they stay resident.

## Commands

- `cargo run` — run the server (equivalent to `cargo run --bin jana2u_pos_backend`; required explicitly if disambiguating from the seed binaries)
- `cargo test` — run all tests. Several `tests/*.rs` files require a reachable MongoDB (see config note below); `tests/openapi_test.rs` and `tests/response_format_test.rs` do not.
- `cargo test --test scenarios_test health_route_returns_success_format` — run a single integration test by name
- `cargo run --bin seed_api_key` — generate and insert a random API key into the `api_keys` collection
- `cargo run --bin seed_providers` — upserts the inventory module's default category/subcategory reference data into the `categories`/`subcategories` collections (see `default_categories` in `src/bin/seed_providers.rs`)
- `cargo run --bin seed_suppliers` — upserts a handful of sample repair/retail suppliers into the `suppliers` collection (see `sample_suppliers` in `src/bin/seed_suppliers.rs`)
- `cargo run --bin seed_customers` — upserts 20 realistic retail/repair/corporate/print customer profiles into the `customers` collection (see `sample_customers` in `src/bin/seed_customers.rs`)
- `cargo run --bin seed_admin` — create-if-missing bootstrap of the first Admin account (requires `SEED_ADMIN_EMAIL`/`SEED_ADMIN_PASSWORD`/`SEED_ADMIN_NAME` to be set — see `.env.example` — fails fast rather than falling back to a hardcoded password), since registration is admin-provisioned only — see **Auth, users & permissions** in `src/CLAUDE.md`
- `make check` (root `Makefile`) — runs `cargo fmt --check`, then `cargo clippy --all-targets --all-features -- -D warnings`, then `cargo test`, stopping at the first failure; `make ci` is an alias for it (the intended CI entrypoint)

**After every change**, run the following and fix anything it reports before considering the work done:

```
make check
# (or: cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test)
```

(`rustfmt`/`clippy` components must be installed — `rustup component add rustfmt clippy` — if either command is missing.)

**Every change must be tested — this is mandatory, not just running the pipeline above.** `cargo test` in that pipeline only re-runs whatever tests already exist; it does not by itself prove a new change works. So for any change:
1. Find the test file that already covers the part of the code you're touching (see the `tests/*.rs` files below) and extend it with a case for the change.
2. If no test file covers that part yet, **create one** — follow the existing naming (`tests/<area>_test.rs`) and pick the right style: no-Mongo unit/integration style (`tests/response_format_test.rs`, `tests/openapi_test.rs`) for anything that doesn't need real data, or the full-stack style (`tests/scenarios_test.rs` via `tests/common::spawn_app()`) for anything that does.
3. Run it and confirm it passes before considering the change done.

Config is loaded from `.env` (via `dotenvy`) in every binary and integration test. Copy `.env.example` to `.env` and point `MONGODB_URI` at a running MongoDB instance before running or testing — `Config::from_env()` fails fast (process exits) on missing/invalid vars, and the Mongo client pings the server at startup, so a bad connection string is caught immediately rather than on first request.

The integration test files, each a different tradeoff between speed and realism:
- `tests/scenarios_test.rs` (+ `tests/common/mod.rs::spawn_app()`) — builds the real router against a **real** MongoDB (no mocking), using `MONGODB_TEST_DB_NAME` (defaults to `jana2u_pos_test`) instead of the dev database so it never touches dev data. Use for anything that actually reads/writes Mongo.
- `tests/inventory_test.rs` / `tests/suppliers_test.rs` / `tests/auth_test.rs` / `tests/users_test.rs` / `tests/reports_test.rs` / etc. — the same `spawn_app()` real-Mongo style as `scenarios_test.rs`, just split into their own files per feature area. Follow this pattern (own `tests/<area>_test.rs` file, `mod common;`, `send`/`send_authed` request helpers, `common::mint_token` for hand-minted role/permission tokens) when a new module needs real-Mongo coverage.
- `tests/openapi_test.rs` — builds the real router with a `mongodb::Client` that is never pinged (still needs `MONGODB_URI` to be a syntactically valid connection string via `.env`, but no live Mongo needed). Asserts the OpenAPI spec lists every module path and Swagger UI serves. Use for anything about routing/docs wiring rather than data.
- `tests/authorization_test.rs` — same no-live-Mongo construction as `openapi_test.rs`. Walks **every** operation in the generated OpenAPI document, sends it an unauthenticated request, and asserts 401 unless the route is on its `PUBLIC_ROUTES` allowlist (health, `POST /auth/login`, and the module-status stubs) — plus the converse, that those public routes still answer, and that a route's `security(...)` annotation matches whether it really enforces auth. This is the guard for the fact that auth is opt-in per handler: a new route registered through `routes!()` is audited the moment it exists, with no edit to this file. If it fails, the fix is almost always a missing `CurrentUser`/`AdminUser` handler argument — add to `PUBLIC_ROUTES` only when a route is genuinely meant to be open, and mirror it in `app::build_router`'s comment.
- `tests/response_format_test.rs` — no router, no Mongo at all; calls `core::response::ApiResponse`/`core::error::AppError` directly and asserts the JSON shape. Use for anything about the response/error envelope itself.

## Route File Organization

Every module's `routes.rs` MUST follow a clean, structured ordering using section banners (`// ============================================================================`) in this exact sequence:
1. `// Router`: `pub fn router() -> OpenApiRouter<AppState>` with `routes!(...)` calls grouped and commented by feature to mirror the file layout.
2. `// Helpers`: Private helper functions for that module (e.g. `parse_object_id`, whitelist mapping, numeric validation).
3. `// <Feature Sections>`: Grouped feature handler blocks (e.g., `// Module status`, `// Products`, `// Stock`, `// Categories`, `// Subcategories`). Read/validation endpoints (such as `/categories/valid`) belong inside their primary domain section next to other reads rather than at the bottom of the file.

## Custom Prefixed Unique Model Keys

Every MongoDB collection document model MUST include a custom unique `key: String` alongside `_id: ObjectId`. Keys follow the format `<prefix>_<nanoid>` (e.g. `prod_x7K9mP2...` for products, `cat_B8nL1q...` for categories, `subcat_...` for subcategories, `sm_9pX2kQ...` for stock movements, `ak_...` for API keys, `sup_...` for suppliers, `splink_...` for supplier-product links, `pur_...` for purchases, `usr_...` for users, `lgs_...` for login sessions, `imp_...` for import batches). Generated via `crate::core::id::generate_id(prefix)` (defined in `core/id.rs`, backed by `nanoid!(16)`). From now onwards, all models, seed scripts, and API domain response payloads MUST include and maintain this `key` field, **and any field on one model that references another collection's document MUST store that document's `key`** — never its display name and never its raw `_id`/`ObjectId` (see `ProductDocument.category_key`/`.subcategory_key` in `modules/inventory/model.rs` for the reference example). The one deliberate exception is a pure internal sequence/counter table like `sku_counters` (`modules/inventory/repository/sku_counter.rs`) — it's not a business entity, so it has no `key`; its `_id` (the derived SKU prefix) already *is* the meaningful key.

## Collection Document Timestamps (`created_at` & `updated_at`)

Every MongoDB collection document model (and its API domain representation) MUST include `created_at` and `updated_at` timestamps (`BsonDateTime` in models, `DateTime<Utc>` in domain structs). When creating documents, set both `created_at` and `updated_at` to the current timestamp (`BsonDateTime::now()`); when updating documents or sub-resources, update `updated_at`. Models must include `#[serde(default = "BsonDateTime::now")]` for deserialization backward compatibility with existing stored documents.

## Code Comments

Every file should make it obvious what it does and why without the reader having to reverse-engineer intent from the code alone. Apply this whenever you touch a file, not just when writing something new:

1. **Module-level banner**: every file — even a 3-line re-export `mod.rs` — opens with a plain `//` comment (not `//!`) stating what it owns and, where relevant, what it deliberately does *not* do. See `domain/inventory.rs` ("no I/O, no Mongo/Axum types...") or `modules/inventory/repository/mod.rs`.
2. **Doc comments (`///`) on `pub`/`pub(crate)` items** whose purpose isn't obvious from the signature alone: every `repository::*` function gets a one-liner naming the query intent, every `service::*` function gets a one-liner naming the business rule it enforces, every `domain`/`model` struct or enum gets a one-liner on non-obvious fields (why a field exists, why it's `Option`, why an enum has the variants it has). Trivial CRUD wrappers still get a short one-liner for consistency — not a paragraph.
3. **Inline comments explain *why*, not *what***: a non-obvious invariant, a workaround, or a decision a future reader would otherwise have to reverse-engineer (e.g. the `$expr`/`$lte` low-stock query shape in `modules/inventory/repository/product.rs`, the cascade-delete-of-subcategories-on-category-delete safety argument in `modules/inventory/service/category.rs`, the DNS resolver workaround in `clients/mongo.rs`). Never restate what a line of code already says — if removing the comment wouldn't confuse a future reader, don't write it.
4. **Reference examples**: `clients/mongo.rs` (DNS resolver workaround), `core/middleware/auth.rs` (why `CurrentUser`/`AdminUser` exist and how they gate routes), `modules/inventory/service/category.rs` (key-based-reference/cascade-delete rationale), `modules/inventory/model.rs` (`CategoryDocument`'s DB-backed-source-of-truth rationale and the `ProductDocument`/`SubcategoryDocument` key-FK pattern), `core/error.rs` (`AppError`'s variant/`Custom`-escape-hatch design).

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
