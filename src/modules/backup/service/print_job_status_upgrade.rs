//! Upgrades a backup taken while the backend still accepted `in_progress` for a print job.

use crate::modules::print_jobs::migration::{CURRENT_STATUS, LEGACY_STATUS};
use serde_json::Value;
use std::collections::HashMap;

/// Rewrites, in memory, every print job row holding the legacy status to the current one,
/// exactly the way `print_jobs::migration::migrate_sqlite` rewrites a live database. Every
/// other row, and every other column, is left as it is.
pub(super) fn upgrade_pre_print_job_status_backup(tables: &mut HashMap<String, Vec<Value>>) {
    for row in tables.get_mut("print_jobs").into_iter().flatten() {
        let Some(status) = row.get_mut("status") else {
            continue;
        };
        if status.as_str() == Some(LEGACY_STATUS) {
            *status = Value::String(CURRENT_STATUS.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_in_progress_print_job_becomes_in_repair_and_nothing_else_changes() {
        let mut tables = HashMap::from([
            (
                "print_jobs".to_string(),
                vec![
                    json!({"key": "prt_a", "status": "in_progress", "quantity": 3}),
                    json!({"key": "prt_b", "status": "received", "quantity": 1}),
                    json!({"key": "prt_c", "status": "in_repair", "quantity": 2}),
                ],
            ),
            (
                "repairs".to_string(),
                vec![json!({"key": "rep_a", "status": "in_progress"})],
            ),
        ]);

        upgrade_pre_print_job_status_backup(&mut tables);

        let statuses: Vec<&str> = tables["print_jobs"]
            .iter()
            .map(|row| row["status"].as_str().expect("a status"))
            .collect();
        assert_eq!(statuses, ["in_repair", "received", "in_repair"]);
        assert_eq!(tables["print_jobs"][0]["quantity"], 3);
        // Another table is never touched, whatever it holds.
        assert_eq!(tables["repairs"][0]["status"], "in_progress");
    }

    #[test]
    fn a_backup_with_no_print_jobs_is_left_alone() {
        let mut tables = HashMap::from([("products".to_string(), vec![json!({"key": "prod_a"})])]);
        upgrade_pre_print_job_status_backup(&mut tables);
        assert_eq!(tables["products"], vec![json!({"key": "prod_a"})]);
    }
}
