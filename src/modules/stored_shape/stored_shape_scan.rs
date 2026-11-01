use super::stored_collection::STORED_COLLECTIONS;
use super::stored_shape_probe::{Fault, repair_document};
use crate::core::error::{AppError, AppResult};
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use std::collections::BTreeMap;

/// Whether `scan` only reports or also writes the missing fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Report,
    Apply,
}

/// What `scan` found in one collection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionShape {
    pub name: &'static str,
    pub scanned: u64,
    /// Documents that lacked at least one field.
    pub incomplete: u64,
    /// How many documents lacked each field, by field name.
    pub missing: BTreeMap<String, u64>,
    /// Documents that do not load for another reason, or that have no backfill value.
    pub unrepairable: u64,
    /// Up to five example reasons for `unrepairable`.
    pub examples: Vec<String>,
}

/// The result of `scan` over every stored collection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShapeReport {
    pub collections: Vec<CollectionShape>,
}

impl ShapeReport {
    /// A readable summary: one line per collection that has incomplete documents, with how
    /// many documents `verb` each field (for example "lack"). Never prints field values.
    #[must_use]
    pub fn render(&self, verb: &str) -> String {
        let mut text = String::new();
        let mut scanned = 0;
        for shape in &self.collections {
            scanned += shape.scanned;
            if shape.incomplete == 0 && shape.unrepairable == 0 {
                continue;
            }
            text.push_str(&format!(
                "  {}: {} of {} documents incomplete\n",
                shape.name, shape.incomplete, shape.scanned
            ));
            for (field, count) in &shape.missing {
                text.push_str(&format!("      {count} {verb} `{field}`\n"));
            }
            if shape.unrepairable > 0 {
                text.push_str(&format!(
                    "      {} cannot be repaired automatically\n",
                    shape.unrepairable
                ));
                for example in &shape.examples {
                    text.push_str(&format!("        e.g. {example}\n"));
                }
            }
        }
        if self.is_clean() {
            text.push_str("  every stored document is complete\n");
        }
        text.push_str(&format!("  documents scanned: {scanned}\n"));
        text
    }

    /// True when every document loads without further changes.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.collections
            .iter()
            .all(|c| c.incomplete == 0 && c.unrepairable == 0)
    }
}

/// Loads every document of every stored collection as its typed struct. In `Apply` mode
/// the missing fields of each incomplete document are written with `$set`, directly on
/// the collection so no tenant stamping or sync metadata changes.
///
/// # Errors
/// Returns an error when the database cannot be read or written.
pub async fn scan(db: &Database, mode: Mode) -> AppResult<ShapeReport> {
    let mut report = ShapeReport::default();
    for stored in STORED_COLLECTIONS {
        let collection = db.collection::<Document>(stored.name);
        let mut cursor = collection.find(doc! {}).await.map_err(AppError::from)?;
        let mut shape = CollectionShape {
            name: stored.name,
            ..CollectionShape::default()
        };
        while let Some(mut document) = cursor.try_next().await.map_err(AppError::from)? {
            shape.scanned += 1;
            if (stored.check)(&document).is_ok() {
                continue;
            }
            let id: Bson = document.get("_id").cloned().unwrap_or(Bson::Null);
            match repair_document(stored.name, &mut document, stored.check) {
                Ok(repair) => {
                    shape.incomplete += 1;
                    for (path, _) in &repair.set {
                        let field = path.rsplit('.').next().unwrap_or(path).to_string();
                        *shape.missing.entry(field).or_insert(0) += 1;
                    }
                    if mode == Mode::Apply {
                        let mut set = Document::new();
                        for (path, value) in repair.set {
                            set.insert(path, value);
                        }
                        collection
                            .update_one(doc! { "_id": id }, doc! { "$set": set })
                            .await
                            .map_err(AppError::from)?;
                    }
                }
                Err(fault) => {
                    shape.unrepairable += 1;
                    if shape.examples.len() < 5 {
                        shape.examples.push(describe(&fault));
                    }
                }
            }
        }
        report.collections.push(shape);
    }
    Ok(report)
}

fn describe(fault: &Fault) -> String {
    match fault {
        Fault::MissingField { field, .. } => format!("missing `{field}`"),
        Fault::Other(reason) => reason.clone(),
    }
}
