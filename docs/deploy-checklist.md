# Deploy checklist

A push to `master` builds and deploys. The GitHub tests run **alongside** the
build and do not block it, so broken code can go live. `make check` on your
machine is the gate.

The exact steps CI runs are in [`ci.yml`](../.github/workflows/ci.yml) and the
[`Makefile`](../Makefile). A check added to `ci.yml` must also be added to
`make check`.

This repo is public: never write a database address, server address or key here.

## Once per machine

- `rustup target add wasm32-unknown-unknown`
- Install Docker Desktop. The tests run against a MongoDB container.
- Keep the `document-server` repo beside this one (`../document-server`); some
  tests read its schemas.

## Before you push

1. Run `rustup update` now and then; CI uses the latest stable Rust.
2. Update `../document-server` (`git pull` on `main`).
3. Commit, then bring in the latest `master` (`git pull`, or merge it into your
   branch).
4. `git status --porcelain` must print nothing.
5. Start Docker Desktop, then run from the repo root:

   ```bash
   make check
   ```

   It starts a throwaway MongoDB (a one-node replica set on port 27027), runs
   the tests against it and removes it. It never uses the database in `.env`.

6. Push **only if `make check` passed**. Do not push past a failure you cannot
   explain.

## Migrations

If the change needs a data migration, check its entry under *Commands* in
[`CLAUDE.md`](../CLAUDE.md) **before pushing**: some run before the deploy, some
after. Use `--dry-run` first.

## After you push

1. Confirm the `deploy` job succeeded: `gh run list --workflow deploy.yml -L 3`,
   then `gh run view <run-id>`. A red run does not mean "not deployed".
2. Open the app and check `/api/health` and the feature you changed.
3. Check the `test` job. If it failed, revert and push:
   `git revert <commit>` (add `-m 1` for a merge commit). The fix goes live only
   after another build. A revert does not undo a migration.

"Run workflow" on GitHub deploys whichever branch you select. Select `master`.

## On Windows

Run everything inside **WSL2**, not PowerShell or Command Prompt. Set it up once
with [Running the checks on Windows](windows-wsl2.md); this checklist then applies
unchanged.

## If the run says the container name is taken

Another run is active, or one was killed before it could clean up. If none is
active: `docker rm -f -v pos-test-mongo`.
