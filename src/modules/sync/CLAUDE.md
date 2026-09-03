# src/modules/sync/ — offline sync

Loaded automatically when working under `src/modules/sync/`. Extracted from the root `CLAUDE.md`.
`../frontend` no longer consumes this module — it exists for a future, separate sync backend — so
this guidance is only relevant when actually touching sync code.

## Offline sync

`../frontend` no longer consumes any of this — its offline-first sync engine was removed and it now
reads/writes straight through the plain REST endpoints below, the same as any ordinary web client.
`modules::sync` (`/sync/changes`, `/sync/status`) is kept fully implemented and untouched: a future,
separate sync backend is planned to reintroduce cross-terminal syncing, and it (or whatever replaces
`../frontend`'s removed engine) is the intended consumer. Several rules below exist for that
consumer's benefit rather than any REST caller's, and remain load-bearing for it even while nothing
currently calls this endpoint.

**The one rule that matters: `/sync/changes` items must be byte-identical to what the REST read
endpoints return.** The client merges the delta feed and the REST snapshot feed into the *same*
local table, so a row that arrives one way must be indistinguishable from the same row arriving
the other way. Serializing the raw Mongo `Document` breaks this — it produces snake_case fields,
`{"$oid"}`/`{"$date"}` wrappers, and omits everything the service layer resolves at read time
(a product's `category`/`subcategory` names; a category's `subcategories`, which live in their own
collection). Each module therefore exposes a `pub(crate) hydrate_sync_documents` that deserializes
into its `*Document` model and runs the existing `into_*` conversion; `sync::service::hydrate`
dispatches to them. `tests/sync_test.rs::sync_changes_items_match_the_rest_dto_shape` is the guard.

- **Cursors** (`modules::sync::cursor`) are URL-safe unpadded base64 of `millis|key`. URL-safe is
  not cosmetic: cursors travel in a query string, and standard base64's `+` is decoded as a space
  by every form-urlencoded parser, which silently corrupts roughly half of all cursors. Standard
  base64 is still accepted on decode so cursors minted before the switch keep working. Anything
  older than `TOMBSTONE_RETENTION_DAYS` is rejected with `CURSOR_INVALID`, since a delta from
  before the retention window would miss deletions.
- **Paging is oldest-first** (`updated_at`, then `key`), which is what makes the cursor resumable.
  It also means `next_cursor` after a one-row read is the *oldest* row, not the newest — so
  `/sync/status` exposes the newest cursor per resource separately. A client adopts that after
  taking a snapshot; deriving one from `/sync/changes` would replay the whole collection.
- **Soft delete only.** A hard-deleted row cannot appear in a delta, so the client would never
  learn it was removed and would resurrect it on the next push. Cursored reads deliberately
  include tombstoned rows (they become the `deleted` key list); cursorless snapshot reads exclude
  them, since the client has nothing to delete yet.
- **Append-only resources are exempt from `deleted_at`.** `stock_movements`, `invoices`, and
  `payments` have no delete route at all — a cancelled invoice flips `status`, it is never removed
  — so their documents never populate `deleted_at` by design, not by oversight. This isn't a gap in
  the rule above: the snapshot/delta filters already treat a permanently-absent field as "not
  deleted," so these collections simply never produce a `deleted` entry. Don't add a tombstone path
  for a resource in this category; add it to `SYNCABLE` and its `hydrate` arm the way `invoices`/
  `payments` were, and leave `deleted_at` off the model entirely.
- **`subcategories` is not a syncable resource.** It is folded into each `categories` item, because
  that is how the client's `Category` type is shaped.
- **Indexes are ensured at startup** (`clients::indexes`). `(updated_at, key)` per syncable
  collection is the exact sort the cursor scan uses — without it every sync call is a collection
  scan, and MongoDB aborts an unindexed in-memory sort past 32 MB, so sync starts failing outright
  as a shop's history grows. The unique `(key, user_id)` on `idempotency_keys` is what makes the
  duplicate-insert branch in `core::middleware::idempotency` reachable at all; without it
  `insert_one` always succeeds and the replay guard silently does nothing.
- **`Idempotency-Key`** is handled globally by `core::middleware::idempotency`. `/api/auth/*` is exempt from capture (`is_idempotency_exempt`): a login response body *is* the issued JWT, so storing it would leave bearer tokens in plaintext in `idempotency_keys.response_body` for anyone replaying the key to read back. A present-but-unverifiable Bearer token is rejected with 401 rather than falling back to the device or `"anonymous"` bucket, so a forged token can't land in a bucket it has no claim to. A concurrent retry
  gets `409 IDEMPOTENCY_IN_PROGRESS` with `Retry-After` — the client treats that as transient, not
  as a conflict. A 5xx removes the in-progress record so the caller can retry cleanly.
- **`If-Match`/`X-Device-Id`** are extracted by `core::middleware::sync_headers`; a version
  mismatch is `409 VERSION_CONFLICT` with the server's current row under `details.server`.
  `X-Server-Time` goes out on every response so the client can detect clock skew.
