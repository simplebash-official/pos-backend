# SimpleBash POS — Backend

[![CI](https://github.com/simplebash-official/pos-backend/actions/workflows/ci.yml/badge.svg)](https://github.com/simplebash-official/pos-backend/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

The REST API behind **SimpleBash POS**, a point-of-sale system for repair and
retail shops: billing, repairs, print jobs, inventory, customers, suppliers,
staff and reports. Written in Rust with [Axum](https://github.com/tokio-rs/axum).

The same binary runs in two very different places:

- **Desktop** — bundled inside the [desktop app](https://github.com/simplebash-official/pos-desktop),
  one shop, **SQLite**, listening only on `127.0.0.1`, fully offline.
- **Web / server** — behind nginx with **MongoDB**, either one shop or many
  shops on one server (`TENANT_MODE=multi`, each shop's data isolated).

## The SimpleBash POS family

| Repository | What it is |
|---|---|
| **pos-backend** (this repo) | REST API — Rust / Axum, SQLite or MongoDB |
| [pos-frontend](https://github.com/simplebash-official/pos-frontend) | The cashier & back-office UI — React / Vite / Mantine |
| [document-server](https://github.com/simplebash-official/document-server) | Renders invoices, receipts, reports and labels to PDF — Rust / Typst |
| [pos-desktop](https://github.com/simplebash-official/pos-desktop) | Windows / macOS / Linux app (Tauri) that bundles all three |

## Features

- **Billing** — sales with split payments (cash, card, online, credit), discounts,
  credit sales with instalments, voids, and **credit notes** for returns. Totals
  are always computed on the server, never trusted from the client.
- **Repairs & print jobs** — job tickets with status tracking, assigned staff and
  commission splits, billed straight into a sale.
- **Inventory** — products with auto-generated SKUs and EAN-13 barcodes, stock
  movements with full history, serial numbers, low-stock alerts, categories and
  subcategories, and spreadsheet import.
- **Customers, suppliers & purchases** — customer accounts and balances,
  supplier catalogues, and purchase receipts that update stock.
- **Staff & access control** — Admin / Manager / Staff roles with fixed
  permission sets, Argon2id password hashing, JWT sessions, login audit log,
  failed-login throttling, and instant revocation when an account is
  deactivated.
- **Reports & analytics** — dashboard KPIs, daily sales, monthly profit, top
  products, inventory valuation, employee commissions, outstanding balances,
  and a cached analytics engine for charts.
- **Documents** — A4 invoices, thermal receipts and credit notes, rendered as
  PDFs by [document-server](https://github.com/simplebash-official/document-server).
- **Desktop extras** — database backup / restore, first-run setup wizard API,
  and device sync to the cloud.
- **API docs built in** — OpenAPI 3 spec and Swagger UI at `/docs`.

## Tech stack

Rust (edition 2024) · Axum 0.8 · Tokio · SQLite (sqlx) · MongoDB driver 3 ·
utoipa (OpenAPI + Swagger UI) · jsonwebtoken · argon2 · tower-http.

## Getting started

### Prerequisites

- [Rust](https://rustup.rs) stable (1.85 or newer; CI and Docker use the latest stable)
- A running [document-server](https://github.com/simplebash-official/document-server)
  for invoice/receipt PDFs (everything else works without it)
- MongoDB — only if you use `DATABASE_TYPE=mongodb`. SQLite needs nothing.

### Run it locally (SQLite)

```bash
git clone https://github.com/simplebash-official/pos-backend.git
cd pos-backend
cp .env.example .env
```

Edit `.env` and set at least:

```dotenv
JWT_SECRET=<output of: openssl rand -hex 32>
DOCUMENT_SERVER_URL=http://localhost:8090
DOCUMENT_SERVER_API_KEY=<the same value as document-server's INTERNAL_API_KEY>
```

Then:

```bash
cargo run --bin seed_all   # optional: demo categories, products, suppliers, customers + an admin
cargo run                  # API on http://localhost:8080
```

Open **http://localhost:8080/docs** for the interactive API reference.

The demo admin created by the seeder is `admin` / `admin@1234` — for
local development only. Set `SEED_ADMIN_PASSWORD` to choose your own; the
server refuses to seed that public default when it is reachable from the
network or `APP_ENV=production`.

### Run it with MongoDB

Copy `.env.web.example` instead, and set `DATABASE_TYPE=mongodb`,
`MONGODB_URI` and `MONGODB_DB_NAME`.

## Configuration

All settings come from environment variables (or `.env`). The server checks
them at startup and refuses to start with a missing or placeholder secret.

| Variable | Default | Purpose |
|---|---|---|
| `DATABASE_TYPE` | `sqlite` | `sqlite` or `mongodb` |
| `DATABASE_URL` | `sqlite://data/pos.db?mode=rwc` | SQLite file (SQLite mode) |
| `MONGODB_URI`, `MONGODB_DB_NAME` | — | Required in MongoDB mode |
| `PORT` / `BIND_ADDR` | `8080` / `0.0.0.0` | Listener. The desktop app uses `127.0.0.1` |
| `JWT_SECRET` | **required** | 32+ random characters; signs login tokens |
| `JWT_EXPIRY_HOURS` | **required** | How long a login lasts |
| `DOCUMENT_SERVER_URL` | **required** | Base URL of document-server |
| `DOCUMENT_SERVER_API_KEY` | **required** | Shared secret, equal to document-server's `INTERNAL_API_KEY` |
| `GENERATED_DOCUMENTS_DIR` | `generated_documents` | Where rendered PDFs are cached |
| `RETURN_WINDOW_DAYS` | `30` | Days after a sale that a return needs no manager override |
| `AUTO_SEED` | `false` | Run all seeders on startup |
| `SEED_ADMIN_PASSWORD` / `SEED_ADMIN_NAME` | — | First admin account for the seeder (its username is always `admin`) |
| `TENANT_MODE` | `single` | `multi` = many shops on one server (MongoDB only) |
| `CORS_ALLOWED_ORIGINS` | desktop + local dev only | Comma-separated browser origins allowed to call the API |
| `IDENTITY_JWKS_URL`, `IDENTITY_ISSUER`, `IDENTITY_TENANT_ID` | — | Optional sign-in through the SimpleBash identity service |
| `PROVISION_SECRET` | — | Multi-tenant only: lets the identity service create shops |
| `APP_ENV` | `development` | `production` tightens seeding rules |
| `RUST_LOG`, `LOG_FORMAT`, `LOG_HTTP_BODIES`, `LOG_SQL` | — | Logging (`LOG_FORMAT=json` is used by the desktop app) |

See [`.env.example`](.env.example), [`.env.desktop.example`](.env.desktop.example)
and [`.env.web.example`](.env.web.example) for annotated templates.

## API overview

Every route lives under `/api/<module>` and returns a consistent envelope:
`{ "success": true, "data": …, "message": … }` on success and
`{ "success": false, "code": "…", "message": "…", "statusCode": … }` on error.

| Module | Prefix |
|---|---|
| Auth & sessions | `/api/auth` |
| Users | `/api/users` |
| Billing (sales, invoices, payments, credit notes) | `/api/billing` |
| Repairs / Print jobs | `/api/repairs`, `/api/print-jobs` |
| Inventory (products, stock, categories) | `/api/inventory` |
| Customers / Employees | `/api/customers`, `/api/employees` |
| Suppliers / Supplier products / Purchases | `/api/suppliers`, `/api/supplier-products`, `/api/purchases` |
| Reports & analytics | `/api/reports` |
| Spreadsheet imports / Number sequences | `/api/imports`, `/api/sequences` |
| System setup, backup, sync | `/api/system`, `/api/backup`, `/api/sync` |

Authentication is a `Authorization: Bearer <token>` header from `POST /api/auth/login`.
Only health, login, the shop-code lookup, first-run setup, a few module-status
stubs and Swagger are public (the internal shop-provisioning route uses its own
shared secret). A test walks every route in the OpenAPI spec and fails if any
other route answers without a token.

A ready-made Postman collection is in [`postman/`](postman).

## Development

```bash
make check          # cargo fmt --check, clippy -D warnings, cargo test
cargo test          # all tests
cargo test --test sqlite_integration_test   # one file
```

The SQLite, OpenAPI and authorization tests need no external services. Tests
that exercise MongoDB need a reachable instance in `MONGODB_URI` — they use
their own database (`MONGODB_TEST_DB_NAME`, default `simplebash_pos_test`),
never your real one. CI runs them against a throwaway MongoDB replica set.

Useful binaries (`cargo run --bin <name>`): `seed_all`, `seed_admin`,
`seed_inventory`, `seed_customers`, `seed_suppliers`, `reset_admin`,
`reset_db`, `create_tenant`.

### Project layout

```
src/
  main.rs         thin binary: config → database → router → serve
  app.rs          router, middleware stack, CORS, Swagger
  core/           config, auth & tenancy middleware, errors, logging, rate limiting
  clients/        SQLite / MongoDB connections, document-server client
  domain/         request/response types shared across modules
  modules/<name>/ one folder per feature: routes.rs → service/ → repository/
  seeds/, bin/    seeders and maintenance tools
tests/            integration tests (one file per area)
```

## Deployment

The [`Dockerfile`](Dockerfile) builds a small, non-root image. Pushes to
`master` build `ghcr.io/simplebash-official/pos-backend:latest` and roll it
out to the SimpleBash server; forks build but never deploy. The desktop app
compiles this crate as a sidecar instead — see
[pos-desktop](https://github.com/simplebash-official/pos-desktop).

## Contributing

Issues and pull requests are welcome. Before opening a PR, run `make check`
and add or extend a test for your change. Please keep new routes in the
OpenAPI spec (`#[utoipa::path]`) and the Postman collection.

## Security

Please report vulnerabilities privately through
[GitHub's "Report a vulnerability"](https://github.com/simplebash-official/pos-backend/security/advisories/new),
not in a public issue.

## License

[GNU Affero General Public License v3.0](LICENSE). If you run a modified
version as a network service, you must make its source available to its users.
