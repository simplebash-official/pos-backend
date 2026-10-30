use super::last_label::LastLabel;

use crate::modules::labels::repository;

use std::collections::HashMap;

/// The latest label per product from history entries ordered newest job first.
/// Copies of every group of that product in that one job are added up (a serial
/// product prints one group per unit).
pub(super) fn last_labels(entries: Vec<repository::LabelledEntry>) -> HashMap<String, LastLabel> {
    let mut latest: HashMap<String, (String, LastLabel)> = HashMap::new();
    for entry in entries {
        match latest.get_mut(&entry.product_key) {
            Some((job_key, last)) if *job_key == entry.job_key => last.copies += entry.copies,
            Some(_) => {}
            None => {
                latest.insert(
                    entry.product_key,
                    (
                        entry.job_key,
                        LastLabel {
                            at: entry.created_at.to_chrono(),
                            copies: entry.copies,
                        },
                    ),
                );
            }
        }
    }
    latest
        .into_iter()
        .map(|(product, (_, last))| (product, last))
        .collect()
}
