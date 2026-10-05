// Static guard for the tenant isolation engine: repositories must reach MongoDB
// only through the tenant-scoped handle (`TenantDatabase::collection`). The few
// places that legitimately need the raw driver handle or a never-filtered
// platform collection are listed here explicitly; anything new fails this test
// until a reviewer adds it to the list on purpose. No database needed.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Raw, unfiltered driver handle (`TenantDatabase::unscoped()`).
const UNSCOPED_ALLOWED: &[&str] = &[
    "src/clients/",
    "src/bin/",
    "src/main.rs",
    // Index creation for the analytics collections.
    "src/modules/reports/engine/mod.rs",
    // The change-stream consumer watches every tenant's writes and re-enters
    // `with_tenant` per event; compaction walks the platform-wide log.
    "src/modules/sync/cloud_capture.rs",
    "src/modules/sync/compaction.rs",
    // The realtime watcher reads every tenant's change-log inserts (read-only)
    // and hands each event only to subscribers of that row's own `tenant_id`.
    "src/modules/sync/live.rs",
];

/// A collection that is never tenant-filtered (`platform_collection`).
const PLATFORM_ALLOWED: &[&str] = &[
    "src/clients/tenant_db.rs",
    // The shop-code directory belongs to no tenant.
    "src/modules/tenants/",
    // The consumer's lease/resume state is per deployment, not per tenant.
    "src/modules/sync/cloud_capture.rs",
];

/// The driver's own `Database` / `Collection` types bypass the wrapper.
const RAW_TYPES_ALLOWED: &[&str] = &["src/clients/", "src/modules/reports/engine/indexes.rs"];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn allowed(relative: &str, list: &[&str]) -> bool {
    list.iter().any(|prefix| relative.starts_with(prefix))
}

fn violations(needle: &dyn Fn(&str) -> bool, list: &[&str]) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let mut found = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if allowed(&relative, list) {
            continue;
        }
        for (number, line) in fs::read_to_string(&file).unwrap().lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if needle(code) {
                found.push(format!("{relative}:{}: {}", number + 1, code.trim()));
            }
        }
    }
    found
}

#[test]
fn only_allowlisted_code_uses_the_unscoped_database_handle() {
    let found = violations(&|l| l.contains(".unscoped()"), UNSCOPED_ALLOWED);
    assert!(
        found.is_empty(),
        "raw `.unscoped()` database access outside the allowlist (use `TenantDatabase::collection`, or add the file to UNSCOPED_ALLOWED on purpose):\n{}",
        found.join("\n")
    );
}

#[test]
fn only_allowlisted_code_uses_platform_collections() {
    let found = violations(&|l| l.contains("platform_collection"), PLATFORM_ALLOWED);
    assert!(
        found.is_empty(),
        "`platform_collection` (never tenant-filtered) outside the allowlist:\n{}",
        found.join("\n")
    );
}

#[test]
fn raw_driver_types_stay_inside_the_client_layer() {
    let found = violations(
        &|l| {
            l.contains("mongodb::Database")
                || l.contains("mongodb::Collection")
                || (l.starts_with("use mongodb::")
                    && (l.contains("Database") || l.contains("Collection")))
        },
        RAW_TYPES_ALLOWED,
    );
    assert!(
        found.is_empty(),
        "raw `mongodb::Database`/`Collection` outside the client layer:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_lint_really_detects_a_violation() {
    // Guard against the scan silently matching nothing: a needle that is known
    // to exist in the allowlisted client layer must be found when nothing is allowed.
    let found = violations(&|l| l.contains(".unscoped()"), &[]);
    assert!(
        !found.is_empty(),
        "the source scan found no `.unscoped()` at all"
    );
}
