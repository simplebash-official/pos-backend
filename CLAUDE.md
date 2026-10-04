# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

The Rust/Axum API backend for **simplebash-pos**, a point-of-sale system for a repair/retail shop (billing, repairs, print jobs, inventory, customers, reports, suppliers). Persistence supports dual database engines: **SQLite** (`DATABASE_TYPE=sqlite`, default, at `pos.db` or via `DATABASE_URL`) and **MongoDB** (`DATABASE_TYPE=mongodb` via `MONGODB_URI`). The sibling `../frontend` (React + Vite + Mantine) is the client this API serves, expecting endpoints under `env.apiBaseUrl` (`/api` by default).

### Keep the two checkouts in sync (check before doing anything)

Every app repo exists **twice** on this machine: the working copy at `pos/<name>` (edit and commit here) and the pinned submodule at `pos/desktop/<name>` (what the desktop app and its installers build from). They drift apart, and when they do, features silently disappear from whichever one you run (the desktop's browser sign-in step once lived only in the pinned frontend, so a run from the working copy showed the old form).

**Before starting any task, compare them:**

```bash
git -C pos/<name> fetch ../desktop/<name>
git -C pos/<name> log --oneline HEAD..FETCH_HEAD     # in the pinned copy, missing here -> merge these in first
git -C pos/<name> log --oneline FETCH_HEAD..HEAD     # in the working copy, missing from the pin
git -C pos/desktop submodule status
```

- Pinned copy has commits the working copy lacks: `git merge <sha>` them into the working copy first (resolve conflicts, rerun the tests).
- Working copy is ahead: commit there, then move the pin: `git -C pos/desktop/<name> fetch ../../<name> && git -C pos/desktop/<name> checkout --detach <sha>`. Never edit files inside `pos/desktop/<name>` directly. Pushing (and the bump PR) is a separate, deliberate step.
- Never run `tauri dev` or build an installer while the working copy is behind the pin.
- Dev run of the desktop app against the local Docker stack: build with `CLOUD_API_URL=http://localhost:8082 CLOUD_SYNC_API_URL=http://localhost:8081` (read at compile time with `option_env!`; `touch pos/desktop/src-tauri/src/cloud/mod.rs` to force a rebuild). Use `:8081` for the Docker POS API: the Docker backend on `:8080` shares its port with the desktop's own sidecar, which answers first on loopback.

## Before finishing any task: test everything that can be tested

A task is not done until everything in this codebase that *can* be tested, and everything related to the feature you implemented, has been tested and passes. Not just the lines you changed.

1. **The feature itself:** add or extend automated tests for every behaviour you added or changed (happy path, failure paths, edge cases), then run them.
2. **Everything around it:** run this repo's full suite and quality gates (below), not only the new tests. A change can break a caller far away.
3. **Across repos:** when a feature spans repos (backend, frontend, desktop shell, document-server, identity-server, app-frontend, deployment), run the gates in *every* repo you touched, plus the tests that exercise the whole path end to end.
4. **For real:** what automated tests cannot reach (UI behaviour, a running stack, long-lived connections, a deploy) is verified by running it: the local Docker stack (`LOCAL_DOCKER_GUIDE.md` at the workspace root), the desktop app, or the browser.
5. **Match CI's toolchain:** CI uses the latest stable Rust/Node. A lint that passes on an older local toolchain can still fail there, so update (`rustup update`) or run the gate with CI's version before calling it done.
6. **Report honestly:** say what you ran and its result. Anything you could not test is named, with the reason, and never presented as passing. A suite that silently skips (e.g. no database configured) is not a passing suite.

**This repo's gate:** `make check` (`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`). The Mongo-backed suites (`sync_cloud_test`, `sync_convergence_test`, `scenarios_test` and the other `spawn_app()` files) must actually run against Atlas via `MONGODB_URI` (never a Docker Mongo). For sync work also run `sync_device_test` and the desktop shell's `cargo test` in `pos/desktop/src-tauri`.

# Where the rest of the guidance lives

Architecture, code-writing conventions, and module-specific rules are split into directory-scoped
`CLAUDE.md` files that load automatically when Claude works under those trees — keeping them out of
context for doc-only / config sessions:

- **`src/CLAUDE.md`** — architecture (binary/library split, request flow, database abstraction, every feature module's
  layering and rationale, `core/`/`domain/`/`clients/`), aggregation-pipeline policy, and
  the dashboard-stats-endpoint pattern.
- **`src/modules/CLAUDE.md`** — the required pattern for adding/changing routes (OpenAPI/`utoipa`
  wiring) and the rule that `postman/backend.postman_collection.json` is updated in the same change.
- **`src/modules/sync/CLAUDE.md`** — the offline-sync module's byte-identical-DTO rule, cursors,
  soft-delete/tombstone semantics, and idempotency/version-conflict middleware.

The invariants below apply everywhere and are easy to violate, so they stay resident.

## Commands

- `cargo run` — run the server (equivalent to `cargo run --bin simplebash_pos_backend`; required explicitly if disambiguating from the seed binaries). Respects `AUTO_SEED=true` to automatically run seeders on startup.
- `cargo test` — run all tests. `tests/sqlite_integration_test.rs`, `tests/seeding_test.rs`, `tests/openapi_test.rs`, and `tests/response_format_test.rs` run without live MongoDB; Mongo-specific integration tests require a reachable MongoDB instance.
- `cargo test --test scenarios_test health_route_returns_success_format` — run a single integration test by name
- `cargo run --bin seed_all` — runs all seeds in dependency order (`admin`, `providers`, `suppliers`, `customers`, `inventory`, `api_key`) across SQLite or MongoDB
- `cargo run --bin seed_admin` — create-if-missing bootstrap of the first Admin account (`admin@pos.com` / `admin@1234`, name: `"System Admin"`). Optional overrides via `SEED_ADMIN_EMAIL`/`SEED_ADMIN_PASSWORD`/`SEED_ADMIN_NAME` env vars.
- `cargo run --bin seed_providers` — upserts default category/subcategory reference data into the `categories`/`subcategories` tables/collections
- `cargo run --bin seed_suppliers` — upserts sample repair/retail suppliers into the `suppliers` table/collection
- `cargo run --bin seed_customers` — upserts 20 realistic retail/repair/corporate/print customer profiles into the `customers` table/collection
- `cargo run --bin seed_inventory` — seeds products, stock movements, and serial numbers into the database
- `cargo run --bin seed_api_key` — generates and inserts a random API key into the `api_keys` table/collection
- `make check` (root `Makefile`) — runs `cargo fmt --check`, then `cargo clippy --all-targets --all-features -- -D warnings`, then `cargo test`, stopping at the first failure; `make ci` is an alias for it (the intended CI entrypoint)

**After every change**, run the following and fix anything it reports before considering the work done:

```
make check
# (or: cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test)
```

(`rustfmt`/`clippy` components must be installed — `rustup component add rustfmt clippy` — if either command is missing.)

**Every change must be tested — this is mandatory, not just running the pipeline above.** `cargo test` in that pipeline only re-runs whatever tests already exist; it does not by itself prove a new change works. So for any change:
1. Find the test file that already covers the part of the code you're touching (see the `tests/*.rs` files below) and extend it with a case for the change.
2. If no test file covers that part yet, **create one** — follow the existing naming (`tests/<area>_test.rs`) and pick the right style: no-Mongo unit/integration style (`tests/response_format_test.rs`, `tests/openapi_test.rs`, `tests/sqlite_integration_test.rs`, `tests/seeding_test.rs`) for anything that doesn't need real data or uses SQLite, or the full-stack style (`tests/scenarios_test.rs` via `tests/common::spawn_app()`) for MongoDB tests.
3. Run it and confirm it passes before considering the change done.

Config is loaded from `.env` (via `dotenvy`) in every binary and integration test. Two dedicated configuration templates are provided:
- `.env.desktop.example`: Configures embedded SQLite (`DATABASE_TYPE=sqlite`), loopback listener (`BIND_ADDR=127.0.0.1`), and local document-server for desktop/Tauri operations.
- `PROVISION_SECRET` (multi-tenant web only, 16+ chars, shared with identity-server) enables `POST /api/internal/provision`; leave it unset everywhere else (the route then 404s). Details in `src/CLAUDE.md` → *Shop provisioning*.
- `.env.web.example`: Configures MongoDB cluster persistence (`DATABASE_TYPE=mongodb`, `MONGODB_URI`), multi-interface listener (`BIND_ADDR=0.0.0.0`), and containerized document-server for web/server deployments.
Default `DATABASE_TYPE=sqlite` requires no external services (creates `pos.db` automatically). If `DATABASE_TYPE=mongodb`, copy `.env.web.example` to `.env` and point `MONGODB_URI` at a running MongoDB instance before running or testing — `Config::from_env()` fails fast (process exits) on missing/invalid vars.

The integration test files, each a different tradeoff between speed and realism:
- `tests/sqlite_integration_test.rs` — tests complete end-to-end POS lifecycle (products, sales, payments, credit notes, stock tracking) directly against in-memory SQLite.
- `tests/seeding_test.rs` — tests seed idempotency, complete database population, and sale transactions against seeded data.
- `tests/scenarios_test.rs` (+ `tests/common/mod.rs::spawn_app()`) — builds the real router against a **real** MongoDB (no mocking), using `MONGODB_TEST_DB_NAME` (defaults to `simplebash_pos_test`) instead of the dev database so it never touches dev data. Use for anything that actually reads/writes Mongo.
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

## Recent Features & Evolution (Last 30 Days)

### Landed Features & Architecture
1. **Dynamic Database Backup & Restore (`src/modules/backup/`)**:
   - `POST /api/backup/export`: Queries `sqlite_master` to dynamically discover and serialize all non-ephemeral application tables to JSON.
   - `POST /api/backup/import`: Executes transactional restore. Drops and repopulates tables with `PRAGMA foreign_keys = OFF;`, escapes column identifiers, restores data, re-enables `PRAGMA foreign_keys = ON;`, and executes `PRAGMA foreign_key_check;` before commit.
   - Schema Evolution: Dynamically filters row fields through `PRAGMA table_info` so obsolete columns in old backups are safely omitted and newly added columns receive their default value.
   - Body Limit: Wrapped with `axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024)` to support 50MB backups.
   - Guarded by `AdminUser` (`role: admin` claim).
2. **Dual Database Engine Support (SQLite + MongoDB)**:
   - Embedded SQLite (`DATABASE_TYPE=sqlite`) is the primary desktop persistence mode; MongoDB (`DATABASE_TYPE=mongodb`) powers server/web deployments.
   - Seeding utilities (`seed_all`, `seed_admin`, `seed_inventory`, etc.) execute seamlessly across both engines.
3. **High-Performance Analytics Engine (`src/modules/reports/`)**:
   - Time-series bucketing with empty-bucket pruning and local timezone offset adjustments.
   - Active cache invalidation via `state.reports_engine.invalidate_active()`.
4. **Returns & Credit Notes Flow (`src/modules/credit_notes/`)**:
   - Returns are tracked as distinct credit note documents with explicit allocations against invoices, preserving immutable sales ledger history.
5. **System Setup & Initial Installation Module (`src/modules/system/`)**:
   - `system_installations` SQLite table tracks `installation_id`, `installed_at`, `app_version`, `platform`, `setup_completed`, `setup_completed_at`, and `sample_data_loaded`.
   - `GET /api/system/setup-status`: Public endpoint returning whether the system has been initialized.
   - `POST /api/system/setup`: Public bootstrap endpoint that initializes the database, creates the primary administrator (`admin@pos.com` / `admin@1234` or custom credentials), seeds API keys, and conditionally populates sample demo data (categories, products, suppliers, customers) if `load_sample_data: true`. If `load_sample_data: false`, leaves all 25 tables completely empty. Issues a JWT token for immediate auto-login.
   - `GET /api/system/installation`: Admin-guarded endpoint to inspect workstation installation metadata.

### Future Implementation Rules
- **New Tables & Dynamic Discovery**: When adding a new table, define it in `src/clients/sqlite_schema.sql` and add it to `src/bin/reset_db.rs`. The backup module dynamically queries `sqlite_master` (excluding `idempotency_keys` and `_sqlx_migrations`), so new tables are automatically backed up without altering backup repository code.
- **Report Cache Invalidation**: Any operation that performs mass mutation or restores historical sales data must call `state.reports_engine.invalidate_active()` to clear live dashboard caches.
- **Route Registration Invariants**:
  - Every route must use `routes!()` with OpenAPI `#[utoipa::path]` annotations.
  - Every new route must be mirrored in `postman/backend.postman_collection.json`.
  - Unauthenticated routes must be explicitly added to `PUBLIC_ROUTES` in `tests/authorization_test.rs`.

### How Agents Can Help
- **Route & Auth Audit**:
  - Run `cargo test --test openapi_test` to verify all route paths are documented in Swagger/OpenAPI.
  - Run `cargo test --test authorization_test` to ensure no handler inadvertently omits `CurrentUser` or `AdminUser`.
- **Database & Backup Testing**:
  - Run `cargo test --test backup_test` when modifying database schemas or backup routines to verify roundtrip export/restore and schema evolution tolerance.
  - Run `cargo test --test sqlite_integration_test` to ensure core POS flows succeed in SQLite mode.
- **Quality Gates**:
  - Always run `make check` (or `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test`) before finalizing any backend changes.

