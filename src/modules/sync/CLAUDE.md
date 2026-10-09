# src/modules/sync/ — sync

Loaded automatically when working under `src/modules/sync/`. Two generations live here:

- **Sync v2 (live)**: desktops (SQLite) ⇄ the multi-tenant cloud (MongoDB) ⇄ the website.
  Desktop side: `outbox`, `apply`, `state`, `routes_local` (driven by the desktop shell's
  agent, `pos-desktop/src-tauri/src/sync/`). Cloud side: `push`, `pull`, `snapshot`,
  `cloud_capture`, `cloud_store`, `routes_v2`. Realtime notifications: `live`.
- **Legacy `/sync/changes` + `/sync/status`** (`routes`, `service`, `cursor`): the old
  per-resource cursor feed, kept for compatibility; see the rules further down.

## Sync v2: how a change travels (and must stay fast)

```
desktop write ─► SQLite triggers ─► sync_outbox ─► (local SSE `outbox`) ─► agent pushes
cloud write (REST or push) ─► change stream ─► cloud_capture ─► sync_changes (per-tenant seq)
sync_changes insert ─► live::run_watcher (every instance) ─► GET /api/sync/events (SSE)
     ├─► other desktops: agent pulls since its cursor ─► apply ─► `sync://applied` ─► UI reloads
     └─► website tabs: invalidate the matching TanStack queries
```

Rules that keep it correct:

- **Every write to a synced collection names its writer.** `updated_by_device` is stamped
  centrally by `clients::tenant_db::ScopedCollection` from the request's origin
  (`core::sync_origin`: device id of a device token, else `web:<X-Device-Id>`, else `cloud`).
  A device's pull skips rows whose origin is itself, so a write that kept the previous writer's
  id would never reach that device. Do not hand-set `updated_by_device` in repositories; use
  `preserve_origin()` only for derived-ledger recomputes (`derived_mongo`) and stock-only moves.
  `core::sync_origin::SYNCED_COLLECTIONS` must equal `SYNC_RESOURCES` tables + `subcategories`
  (guarded by a test in `resources.rs`).
- **No silent hard deletes.** A deleted document leaves the change stream nothing to read.
  Prefer a soft delete; where a repository must remove a row of a synced collection outright,
  call `sync::service::record_hard_deletes` right after (a `sync_tombstones` marker the
  consumer publishes as a delete). Subcategory writes bump their parent category
  (`touch_parent_category`) because they travel folded into it.
- **Derived columns never move last-writer-wins.** A write that only changes `derived`
  columns (`products.stock_quantity`) must not bump `updated_at`/`version`; the SQLite products
  trigger only fires on non-derived columns and the consumer skips derived-only updates.
- **A lost stream position is recovered, not retried forever.** If the server can no longer resume
  from the saved token (`ChangeStreamHistoryLost`, e.g. the deployment was down longer than the
  oplog window; also invalid/undecodable tokens, `UNUSABLE_POSITION_CODES`), `consume` opens a fresh
  stream first and then `catch_up`s: every document of a syncable collection updated since
  `resume_token_at` (saved with each token, minus 5 min) and every hard-delete marker is
  re-published under a `catchup:` `source_token`. Duplicates are harmless (a change applies
  idempotently by version). Without that, the consumer logged the same error every ~1.5 s forever and
  captured nothing. Test: `sync_cloud_test::a_lost_stream_position_is_recovered_and_missed_writes_are_replayed`.
- **The consumer never skips an event.** `cloud_capture` retries, and on persistent failure
  restarts from its last saved resume token (an event failing across 3 restarts is logged and
  dropped so one bad write cannot stop every tenant's feed). Duplicates are absorbed by the
  unique `source_token`; an unused `seq` is just a gap.
- **Realtime is notification only.** SSE events carry `{seq, resource, origin}`, never record
  data; clients read through `GET /sync/pull` or REST, so permissions and DTO shapes stay put.
  Polling remains the safety net (desktop timer 60 s while both streams are up, 15 s otherwise).
  nginx must not buffer `/api/sync/events` (see `pos-compose` `nginx/conf.d/default.conf`).

## Legacy `/sync/changes` feed

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
