# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

The Rust/Axum API backend for **simplebash-pos**, a point-of-sale system for a repair/retail shop (billing, repairs, print jobs, inventory, customers, reports, suppliers). Persistence supports dual database engines: **SQLite** (`DATABASE_TYPE=sqlite`, at the file named by `DATABASE_URL`) and **MongoDB** (`DATABASE_TYPE=mongodb` via `MONGODB_URI`). The sibling `../frontend` (React + Vite + Mantine) is the client this API serves, expecting endpoints under `env.apiBaseUrl` (`/api` by default).

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

**STRICT: after every implementation (every feature, fix or refactor, however small) AND again before every `git commit` in this repo, `make check-atlas` must pass. Do not call a task done, and never commit, and never ask to push, on a red or unrun result.** It runs clippy with CI's flags, then the whole `cargo test --no-fail-fast` against Atlas on a fresh `jtcheck_<hex>` database (`scripts/check-atlas.sh`, URI read from the workspace-root `.env.docker.local`, database dropped afterwards). Plain `make check` is not enough: without `MONGODB_URI` the Mongo-backed suites do not really run, so failures such as the sync codec round-trip (`sync_cloud_test`) only appeared in CI after a push. When a domain model gains a read-time (never stored) field, add it to `enrichment_fields` in `modules/sync/mongo_codec.rs` in the same change. State the `make check-atlas` result when reporting.

**Run all of these before you say a backend task is finished** (a feature once reached CI with 12 clippy errors, a Staff token able to list login sessions, and undocumented routes, none of which a partial run would have caught):

1. `cargo clippy --all-targets --all-features -- -D warnings` on the current stable Rust (`rustup update` first). This is exactly what CI runs; `cargo clippy --fix --allow-dirty` clears most of it. Do not rely on `cargo build` or `cargo test` alone: clippy has its own errors.
2. The **whole** `cargo test` (use `--no-fail-fast` so one failing binary does not hide the rest), against Atlas, on a **fresh throwaway database**: `MONGODB_DB_NAME=jt<area>_<20 hex> MONGODB_TEST_DB_NAME=<same>`. The harness refuses any test database name that does not start with `jt`, and a shared database accumulates data, so tests such as the reminders ones then fail for reasons that have nothing to do with your change.
3. For every new or changed route: a `security(("bearerAuth" = []))` entry in its `#[utoipa::path]` when it needs a token (`authorization_test` checks it), and its permission checks tested for each role.
4. A new permission gets its own constant in `core/constants/permissions.rs`; never reuse an existing permission string for a different feature (`sessions:view` guards the login-session list, register shifts use `register_sessions:*`). Re-check what `ADMIN`/`MANAGER`/`STAFF_PERMISSIONS` now grant.
5. `cargo fmt --all --check` (`max_width = 100`, the workspace `rustfmt.toml`) is enforced in CI and in `make check`; run `cargo fmt --all` before you finish.
6. After pushing, watch the run (`gh run list --limit 3`, `gh run view <id> --log-failed`) until it is green. Pushing is a separate, deliberate step, so ask first.

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

- `cargo run` — run the server (equivalent to `cargo run --bin simplebash_pos_backend`; required explicitly if disambiguating from the seed binaries). Runs the seeders on startup when `AUTO_SEED=true`.
- `cargo test` — run all tests. `tests/sqlite_integration_test.rs`, `tests/seeding_test.rs`, `tests/openapi_test.rs`, and `tests/response_format_test.rs` run without live MongoDB; Mongo-specific integration tests require a reachable MongoDB instance.
- `cargo test --test scenarios_test health_route_returns_success_format` — run a single integration test by name
- `cargo run --bin seed_all` — runs all seeds in dependency order (`admin`, `providers`, `cash_categories`, `suppliers`, `customers`, `inventory`, `api_key`) across SQLite or MongoDB
- `cargo run --bin seed_admin` — create-if-missing bootstrap of the first Admin account (username `admin`; the name and password come from the required `SEED_ADMIN_NAME` / `SEED_ADMIN_PASSWORD`, which the config refuses when they are the old documented default).
- `cargo run --bin seed_providers` — upserts default category/subcategory reference data into the `categories`/`subcategories` tables/collections
- `cargo run --bin seed_suppliers` — upserts sample repair/retail suppliers into the `suppliers` table/collection
- `cargo run --bin seed_customers` — upserts 20 realistic retail/repair/corporate/print customer profiles into the `customers` table/collection
- `cargo run --bin seed_inventory` — seeds products, stock movements, and serial numbers into the database
- `cargo run --bin seed_api_key` — generates and inserts a random API key into the `api_keys` table/collection
- `cargo run --bin migrate_cash_movements [-- --dry-run]` — one-time MongoDB migration of old cash movements to typed movements with categories (also seeds every shop's built-in cash categories). Idempotent; run it before deploying the typed-movement backend to an environment with existing data. SQLite migrates itself at startup.
- `cargo run --bin migrate_drawer_by_user [-- --dry-run]` — one-time MongoDB migration of `drawer_state` from one balance per machine to one per user (the shift owner; when one user had several machines the newest row wins, a machine that never had a shift is dropped). The dry run prints every planned balance; review it, then apply right after deploying the per-user drawer backend. Idempotent. SQLite migrates itself at startup (`migrate_schema`).
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
- `PROVISION_SECRET` (multi-tenant web only, 16+ chars, shared with identity-server) is required when `PROVISIONING=enabled` and enables `POST /api/internal/provision`; every other deployment sets `PROVISIONING=disabled` (the route then 404s). Details in `src/CLAUDE.md` → *Shop provisioning*.
- `.env.web.example`: Configures MongoDB cluster persistence (`DATABASE_TYPE=mongodb`, `MONGODB_URI`), multi-interface listener (`BIND_ADDR=0.0.0.0`), and containerized document-server for web/server deployments.
`DATABASE_TYPE=sqlite` needs no external services (it creates the file named by `DATABASE_URL`). If `DATABASE_TYPE=mongodb`, copy `.env.web.example` to `.env` and point `MONGODB_URI` at a running MongoDB instance before running or testing — `Config::from_env()` fails fast (process exits) on missing/invalid vars.

The integration test files, each a different tradeoff between speed and realism:
- `tests/sqlite_integration_test.rs` — tests complete end-to-end POS lifecycle (products, sales, payments, credit notes, stock tracking) directly against in-memory SQLite.
- `tests/seeding_test.rs` — tests seed idempotency, complete database population, and sale transactions against seeded data.
- `tests/scenarios_test.rs` (+ `tests/common::spawn_app()`) — builds the real router against a **real** MongoDB (no mocking), using the required `MONGODB_TEST_DB_NAME` (a throwaway `jt*` database) instead of the dev database so it never touches dev data. Use for anything that actually reads/writes Mongo.
- `tests/inventory_test.rs` / `tests/suppliers_test.rs` / `tests/auth_test.rs` / `tests/users_test.rs` / `tests/reports_test.rs` / etc. — the same `spawn_app()` real-Mongo style as `scenarios_test.rs`, just split into their own files per feature area. Follow this pattern (own `tests/<area>_test.rs` file, or a `tests/<area>_test/` directory with `main.rs`, `helpers.rs` and one module per topic once it passes about 1,000 lines — `main.rs` declares `#[path = "../common/mod.rs"] mod common;` — `send`/`send_authed` request helpers, `common::mint_token` for hand-minted role/permission tokens) when a new module needs real-Mongo coverage.
- `tests/openapi_test.rs` — builds the real router with a `mongodb::Client` that is never pinged (still needs `MONGODB_URI` to be a syntactically valid connection string via `.env`, but no live Mongo needed). Asserts the OpenAPI spec lists every module path and Swagger UI serves. Use for anything about routing/docs wiring rather than data.
- `tests/authorization_test.rs` — same no-live-Mongo construction as `openapi_test.rs`. Walks **every** operation in the generated OpenAPI document, sends it an unauthenticated request, and asserts 401 unless the route is on its `PUBLIC_ROUTES` allowlist (health, `POST /auth/login`, and the module-status stubs) — plus the converse, that those public routes still answer, and that a route's `security(...)` annotation matches whether it really enforces auth. This is the guard for the fact that auth is opt-in per handler: a new route registered through `routes!()` is audited the moment it exists, with no edit to this file. If it fails, the fix is almost always a missing `CurrentUser`/`AdminUser` handler argument — add to `PUBLIC_ROUTES` only when a route is genuinely meant to be open, and mirror it in `app::build_router`'s comment.
- `tests/calc_test.rs` — live calculations over HTTP on SQLite (they write nothing, so no Mongo): permissions, range checks, and the golden request/reply pairs in `tests/golden/calc/` that the frontend contract is built on (`UPDATE_GOLDEN=1` re-records them; review the diff).
- `tests/response_format_test.rs` — no router, no Mongo at all; calls `core::response::ApiResponse`/`core::error::AppError` directly and asserts the JSON shape. Use for anything about the response/error envelope itself.

## Rust style (Google Rust Style Guide) and file layout

The Google Rust Style Guide governs all Rust here (the `rust-style-guide` skill has the full text); where it is silent, the official Rust style guide and API guidelines apply. What is binding and easy to get wrong:

- **One module per `use`, no globs.** `use a::{B, C}` is fine; `use a::{B, c::D}` is not. `use super::*` only inside an inline `#[cfg(test)] mod tests`. Call free functions through their module (`fs::read`), import types and traits unqualified. Import a trait inside the function that needs it unless the whole file uses it.
- **Files are named after their primary item; non-leaf modules hold only `mod` lines and `pub use` re-exports** (`mod.rs` never contains code). A module is a directory with `mod.rs` plus `<feature>_routes.rs`, `<feature>_service.rs`, one file per large item or per cohesive group of small related items. Helpers live in the file of their one user. Grouping closely related small items in one file is fine; one item per file is not a rule. Aim for files under about 600 lines. `routes` / `service` / `repository` / `model` / `utils` are directory and layer names, never file names.
- **Layering is unchanged:** routes (HTTP only) -> service (business rules) -> repository (persistence only). `repository` and `service` stay private to the module; other modules reach in through `pub(crate)` service functions.
- **No fallbacks, no defaults.** Never `unwrap_or`, `unwrap_or_default` or `.ok()` to hide a failure or invent a value: a missing or corrupt stored value is an error (`AppError::internal`), a missing required setting stops the process at startup, and a request field that is absent stays `Option` and is mapped explicitly. The allowed exceptions are written down where they apply: the zero of an empty set (a sum over no rows), the documented empty-form value in `pos-calc-core`, pagination limits as named constants, and the legacy-shape readers in the old-backup upgrader and the one-time migrations.
- **Configuration has no defaults.** Every variable in `core/config` is required; a missing or invalid one exits with its name. Nothing outside `core/config` reads `std::env` (the `src/bin` tools read config through `Config` too). Tests build their configuration in `tests/common/test_config.rs`.
- **Stored data must be complete.** No `#[serde(default)]` on a stored `*Document` (or on anything embedded in one): a missing field makes the document fail to load, naming the field; a genuinely optional field is an `Option` (serde reads a missing `Option` as `None`, no attribute needed). Request DTOs may keep a default only where it is documented API behaviour (`confirm_duplicate` false, an empty `tags` list). When a field is added to a stored document, the same change adds its backfill to `modules::stored_shape` (`BACKFILLABLE` and `backfill_value`), because documents written before it lack it. **Rollout order:** back up, `cargo run --bin check_data_shape` (read only; lists which documents lack which fields), `cargo run --bin migrate_stored_defaults -- --dry-run`, then without `--dry-run`, `check_data_shape` again (must be clean), and only then deploy the new backend. The tools write straight to the collections (no tenant stamping, no sync metadata). SQLite needs nothing: rows are read column by column with required readers, and old backups restore into a schema whose column defaults are storage, not reader, fallbacks.
- **Errors and panics:** `Result`/`Option` for failure, `expect("why this cannot fail")` instead of `unwrap()` (tests included), never recover a poisoned mutex (`.lock().expect("<name> mutex poisoned")`), no `let _ =` on a fallible call unless it is documented best-effort cleanup.
- **Types:** `#[derive(Debug)]` on everything (manual redacting impls for anything holding a secret), `#[must_use]` on pure functions whose result matters, named structs instead of tuples returned from public functions, enums instead of bool parameters, `&[T]` / `&str` instead of `&Vec<T>` / `&String`, `#[non_exhaustive]` on public enums that other crates match on (and list the variants you care about plus a wildcard that fails loudly).
- **Docs:** every module starts with `//!` (what it owns, what it deliberately does not do); every `pub` item gets a `///` that adds something the name does not (a repository function names the query intent, a service function the business rule, a struct the non-obvious field). `//` is for implementation notes explaining *why*. Do not restate the code.
- **Format:** `cargo fmt --all` with the workspace `max_width = 100`. `cargo clippy --workspace --all-targets --all-features -- -D warnings` on current stable.

### Route files
A module's routes file declares `pub fn router() -> OpenApiRouter<AppState>` (the `routes!(...)` list, grouped and commented by feature) and the handlers, each with its `#[utoipa::path]`. Large modules put one handler or one cohesive group of handlers per file under `routes/`, with `<module>_routes.rs` holding the router. Read and validation endpoints (such as `/categories/valid`) sit with their domain's other reads.

## Custom Prefixed Unique Model Keys

Every MongoDB collection document model MUST include a custom unique `key: String` alongside `_id: ObjectId`. Keys follow the format `<prefix>_<nanoid>` (e.g. `prod_x7K9mP2...` for products, `cat_B8nL1q...` for categories, `subcat_...` for subcategories, `sm_9pX2kQ...` for stock movements, `ak_...` for API keys, `sup_...` for suppliers, `splink_...` for supplier-product links, `pur_...` for purchases, `usr_...` for users, `lgs_...` for login sessions, `imp_...` for import batches). Generated via `crate::core::id::generate_id(prefix)` (defined in `core/id.rs`, backed by `nanoid!(16)`). From now onwards, all models, seed scripts, and API domain response payloads MUST include and maintain this `key` field, **and any field on one model that references another collection's document MUST store that document's `key`** — never its display name and never its raw `_id`/`ObjectId` (see `ProductDocument.category_key`/`.subcategory_key` in `modules/inventory/model.rs` for the reference example). The one deliberate exception is a pure internal sequence/counter table like `sku_counters` (`modules/inventory/repository/sku_counter.rs`) — it's not a business entity, so it has no `key`; its `_id` (the derived SKU prefix) already *is* the meaningful key.

## Collection Document Timestamps (`created_at` & `updated_at`)

Every MongoDB collection document model (and its API domain representation) MUST include `created_at` and `updated_at` timestamps (`BsonDateTime` in models, `DateTime<Utc>` in domain structs). When creating documents, set both `created_at` and `updated_at` to the current timestamp (`BsonDateTime::now()`); when updating documents or sub-resources, update `updated_at`. Models carry **no** `#[serde(default)]`: a stored document must really have every field (see "Stored data must be complete" below).

## Code Comments

See **Rust style** above for the `//!` / `///` / `//` rules. Reference examples of comments that explain *why*: `clients/mongo.rs` (DNS resolver workaround), `core/middleware/auth/` (why `CurrentUser`/`AdminUser` exist and how they gate routes), `modules/inventory/service/category/` (key-based-reference and cascade-delete rationale), `core/error/` (`AppError`'s variant and `Custom` escape-hatch design).

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
   - `POST /api/system/setup`: Public bootstrap endpoint that initializes the database, creates the primary administrator (username `admin`, with the password the caller supplies), seeds API keys, and conditionally populates sample demo data (categories, products, suppliers, customers) if `load_sample_data: true`. If `load_sample_data: false`, leaves all 25 tables completely empty. Issues a JWT token for immediate auto-login.
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

